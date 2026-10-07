//! Step 2 smoke test: create a collection through the mpl-core CPI and read the
//! account back with Borsh. Proves the no-anchor-feature mpl-core path works
//! end to end against the real program binary.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mpl_core::{accounts::BaseCollectionV1, ID as MPL_CORE_ID},
    solana_awesome::{
        keypair::Keypair,
        message::{Message, VersionedMessage},
        signer::Signer,
        transaction::versioned::VersionedTransaction,
    },
};

const NAME: &str = "Test Collection";
const URI: &str = "https://example.com/collection.json";

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

#[test]
fn creates_collection_via_mpl_core_cpi() {
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
    svm.add_program(
        MPL_CORE_ID,
        include_bytes!("fixtures/mpl_core_program.so"),
    )
    .unwrap();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();

    let collection =
        Pubkey::find_program_address(&[nft_staking::COLLECTION, payer.pubkey().as_ref()], &program_id).0;
    let update_authority =
        Pubkey::find_program_address(&[nft_staking::UPDATE_AUTHORITY, collection.as_ref()], &program_id).0;

    let instruction = Instruction::new_with_bytes(
        program_id,
        &nft_staking::instruction::CreateCollection {
            name: NAME.to_string(),
            uri: URI.to_string(),
        }
        .data(),
        nft_staking::accounts::CreateCollection {
            payer: payer.pubkey(),
            collection,
            update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    send_ok(&mut svm, &[&payer], &[instruction]);

    let account = svm.get_account(&collection).expect("collection account exists");
    let collection_state =
        BaseCollectionV1::from_bytes(&account.data).expect("valid BaseCollectionV1");
    assert_eq!(collection_state.name, NAME);
    assert_eq!(collection_state.uri, URI);
    assert_eq!(collection_state.update_authority.to_bytes(), update_authority.to_bytes());
}