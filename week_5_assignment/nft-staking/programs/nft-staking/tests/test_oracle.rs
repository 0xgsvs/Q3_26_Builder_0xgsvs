//! Oracle external plugin adapter: a time-based transfer gate.
//!
//! An `Oracle` PDA per collection stores the `Transfer` lifecycle
//! validation result (Approved inside the UTC open window, Rejected
//! outside). A permissionless crank updates it. The collection carries an
//! Oracle external plugin adapter with a reject-only capability, so mpl-core
//! blocks asset transfers while the oracle says Rejected.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mpl_core::ID as MPL_CORE_ID,
    solana_awesome::{
        keypair::Keypair,
        message::{Message, VersionedMessage},
        signer::Signer,
        transaction::versioned::VersionedTransaction,
    },
    solana_clock::Clock,
};

fn send_ok(
    svm: &mut LiteSVM,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> litesvm::types::TransactionMetadata {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    match svm.send_transaction(tx) {
        Ok(meta) => meta,
        Err(failed) => panic!("transaction failed:\n{}", failed.meta.pretty_logs()),
    }
}

fn pda(seeds: &[&[u8]], program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program_id).0
}

fn warp(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

struct Env {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    collection: Pubkey,
    update_authority: Pubkey,
    oracle: Pubkey,
    vault: Pubkey,
}

fn setup() -> Env {
    let program_id = nft_staking::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    svm.add_program(
        program_id,
        include_bytes!(concat!(
            env!("CARGO_TARGET_TMPDIR"),
            "/../deploy/nft_staking.so"
        )),
    )
    .unwrap();
    svm.add_program(MPL_CORE_ID, include_bytes!("fixtures/mpl_core_program.so"))
        .unwrap();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();

    let collection = pda(&[nft_staking::COLLECTION, payer.pubkey().as_ref()], &program_id);
    let update_authority = pda(&[nft_staking::UPDATE_AUTHORITY, collection.as_ref()], &program_id);
    let oracle = pda(&[nft_staking::ORACLE, collection.as_ref()], &program_id);
    let vault = pda(&[nft_staking::ORACLE_VAULT, collection.as_ref()], &program_id);

    let mut env = Env {
        svm,
        program_id,
        payer,
        collection,
        update_authority,
        oracle,
        vault,
    };

    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::CreateCollection {
            name: "C".into(),
            uri: "u".into(),
        }
        .data(),
        nft_staking::accounts::CreateCollection {
            payer: env.payer.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    env
}

fn read_oracle(env: &Env) -> nft_staking::state::Oracle {
    let account = env.svm.get_account(&env.oracle).expect("oracle exists");
    let mut data: &[u8] = &account.data;
    anchor_lang::AccountDeserialize::try_deserialize(&mut data).unwrap()
}

/// Reading the oracle account, the mpl-core program must see `Anchor`-offset
/// (8) `OracleValidation::V1` bytes: [1, create, transfer, burn, update].
fn oracle_validation_bytes(env: &Env) -> [u8; 5] {
    let account = env.svm.get_account(&env.oracle).expect("oracle exists");
    account.data[8..13].try_into().unwrap()
}

#[test]
fn init_oracle_creates_account_with_anchor_offset_validation() {
    let mut env = setup();
    warp(&mut env.svm, 1_700_000_000); // arbitrary base time

    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::InitOracle {}.data(),
        nft_staking::accounts::InitOracle {
            payer: env.payer.pubkey(),
            collection: env.collection,
            oracle: env.oracle,
            vault: env.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // mpl-core reads the 5 OracleValidation bytes at Anchor offset (8).
    // Variant V1 = 1, then create/transfer/burn/update results. The oracle is
    // created Rejected (closed) so nothing can slip through before the crank.
    assert_eq!(oracle_validation_bytes(&env), [1, 2, 1, 2, 2]);

    let oracle = read_oracle(&env);
    assert_eq!(oracle.variant, 1);
    assert_eq!(oracle.transfer, 1, "created Rejected");
}