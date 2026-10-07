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
    mpl_core::{
        accounts::BaseCollectionV1,
        fetch_wrapped_external_plugin_adapter,
        types::{
            ExternalPluginAdapter, ExternalPluginAdapterKey, HookableLifecycleEvent,
            ValidationResultsOffset,
        },
        ID as MPL_CORE_ID,
    },
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

/// A unix timestamp whose UTC hour is `hour` (0..24).
fn ts_at_hour(hour: i64) -> i64 {
    1_700_000_000 / 86_400 * 86_400 + hour * 3_600
}

fn init_oracle(env: &mut Env) {
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
}

fn create_asset(env: &mut Env, asset: &Keypair) {
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::CreateAsset {
            name: "A".into(),
            uri: "u".into(),
        }
        .data(),
        nft_staking::accounts::CreateAsset {
            payer: env.payer.pubkey(),
            asset: asset.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer, asset], &[ix]);
}

fn transfer_asset(env: &mut Env, asset: &Pubkey, new_owner: &Pubkey) {
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::TransferAsset {}.data(),
        nft_staking::accounts::TransferAsset {
            owner: env.payer.pubkey(),
            asset: *asset,
            collection: env.collection,
            oracle: env.oracle,
            new_owner: *new_owner,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);
}

/// Calls the crank with `crank` as the reward recipient. The fee payer stays
/// `env.payer`, so the crank's balance delta is exactly the reward.
fn update_oracle_as(env: &mut Env, crank: &Pubkey) {
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::UpdateOracle {}.data(),
        nft_staking::accounts::UpdateOracle {
            crank: *crank,
            collection: env.collection,
            oracle: env.oracle,
            vault: env.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);
}

fn update_oracle(env: &mut Env) {
    let crank = env.payer.pubkey();
    update_oracle_as(env, &crank);
}

/// Credits the oracle vault so it can pay crank rewards.
fn fund_vault(env: &mut Env, lamports: u64) {
    let ix = anchor_lang::solana_program::system_instruction::transfer(
        &env.payer.pubkey(),
        &env.vault,
        lamports,
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);
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

    init_oracle(&mut env);

    // mpl-core reads the 5 OracleValidation bytes at Anchor offset (8).
    // Variant V1 = 1, then create/transfer/burn/update results. The oracle is
    // created Rejected (closed) so nothing can slip through before the crank.
    assert_eq!(oracle_validation_bytes(&env), [1, 2, 1, 2, 2]);

    let oracle = read_oracle(&env);
    assert_eq!(oracle.variant, 1);
    assert_eq!(oracle.transfer, 1, "created Rejected");
}

#[test]
fn crank_rejects_outside_open_hours() {
    let mut env = setup();
    init_oracle(&mut env);

    // 03:00 UTC is outside the 09:00-17:00 window.
    warp(&mut env.svm, ts_at_hour(3));
    update_oracle(&mut env);

    let oracle = read_oracle(&env);
    assert_eq!(oracle.transfer, 1, "Rejected outside hours");
    assert_eq!(oracle.last_hour, 3);
    // mpl-core-visible bytes also reflect the rejection.
    assert_eq!(oracle_validation_bytes(&env)[2], 1);
}

#[test]
fn crank_approves_inside_open_hours() {
    let mut env = setup();
    init_oracle(&mut env);

    // 12:00 UTC is inside the window; and 17:00 exactly is outside (half-open).
    warp(&mut env.svm, ts_at_hour(12));
    update_oracle(&mut env);
    assert_eq!(read_oracle(&env).transfer, 0, "Approved at 12:00");
    assert_eq!(oracle_validation_bytes(&env)[2], 0);

    warp(&mut env.svm, ts_at_hour(17));
    update_oracle(&mut env);
    assert_eq!(read_oracle(&env).transfer, 1, "Rejected at 17:00");
}

#[test]
fn reward_only_near_boundary() {
    let mut env = setup();
    init_oracle(&mut env);
    fund_vault(&mut env, 10_000_000);

    let crank = Keypair::new().pubkey();
    env.svm.airdrop(&crank, 1_000_000).unwrap();
    let before = env.svm.get_account(&crank).unwrap().lamports;

    // 12:00, far from any boundary: state updates but no reward.
    warp(&mut env.svm, ts_at_hour(12));
    update_oracle_as(&mut env, &crank);
    assert_eq!(read_oracle(&env).transfer, 0);
    assert_eq!(
        env.svm.get_account(&crank).unwrap().lamports,
        before,
        "no reward away from a boundary"
    );

    // 09:00:00 (the open boundary): reward is paid.
    let boundary = ts_at_hour(9);
    warp(&mut env.svm, boundary);
    update_oracle_as(&mut env, &crank);
    assert_eq!(
        env.svm.get_account(&crank).unwrap().lamports,
        before + nft_staking::ORACLE_REWARD,
        "rewarded on the open boundary"
    );
}

#[test]
fn collection_carries_reject_only_transfer_oracle_adapter() {
    let env = setup();

    let account = env.svm.get_account(&env.collection).unwrap();
    let collection = mpl_core::Collection::from_bytes(&account.data).unwrap();
    assert_eq!(collection.external_plugin_adapter_list.oracles.len(), 1);
    let oracle_plugin = &collection.external_plugin_adapter_list.oracles[0];
    assert_eq!(oracle_plugin.base_address, env.oracle);
    assert_eq!(oracle_plugin.results_offset, ValidationResultsOffset::Anchor);

    // Build an AccountInfo so we can read the adapter's lifecycle checks from
    // the registry record.
    let mut lamports = account.lamports;
    let mut data = account.data.clone();
    let owner = account.owner;
    let info = anchor_lang::solana_program::account_info::AccountInfo::new(
        &env.collection,
        false,
        false,
        &mut lamports,
        &mut data,
        &owner,
        false,
    );
    let key = ExternalPluginAdapterKey::Oracle(env.oracle);
    let (record, adapter) =
        fetch_wrapped_external_plugin_adapter::<BaseCollectionV1>(&info, None, &key).unwrap();

    assert!(matches!(adapter, ExternalPluginAdapter::Oracle(_)));
    let checks = record.lifecycle_checks.unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].0, HookableLifecycleEvent::Transfer as u8);
    // reject-only: can_listen = false, can_approve = false, can_reject = true.
    assert_eq!(checks[0].1.flags, 0b100);
}

#[test]
fn transfer_succeeds_inside_open_hours() {
    let mut env = setup();
    let asset = Keypair::new();
    create_asset(&mut env, &asset);
    init_oracle(&mut env);

    // Open the gate.
    warp(&mut env.svm, ts_at_hour(12));
    update_oracle(&mut env);

    let recipient = Keypair::new();
    env.svm.airdrop(&recipient.pubkey(), 1_000_000).unwrap();
    transfer_asset(&mut env, &asset.pubkey(), &recipient.pubkey());

    let account = env.svm.get_account(&asset.pubkey()).unwrap();
    let asset_state = mpl_core::accounts::BaseAssetV1::from_bytes(&account.data).unwrap();
    assert_eq!(asset_state.owner, recipient.pubkey(), "asset transferred");
}