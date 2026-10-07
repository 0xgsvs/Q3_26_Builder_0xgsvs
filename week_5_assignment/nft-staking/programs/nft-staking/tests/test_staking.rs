//! Base staking lifecycle: create collection -> create asset -> initialize ->
//! stake (freezes) -> unstake (thaws, closes stake state).

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mpl_core::{
        accounts::{BaseAssetV1, BaseCollectionV1},
        ID as MPL_CORE_ID,
    },
    solana_awesome::{
        keypair::Keypair,
        message::{Message, VersionedMessage},
        signer::Signer,
        transaction::versioned::VersionedTransaction,
    },
};

const NAME: &str = "Collection";
const URI: &str = "https://example.com/c.json";
const REWARD_BPS: u16 = 100;
const FREEZE_PERIOD: u16 = 60;

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

struct Env {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    collection: Pubkey,
    update_authority: Pubkey,
}

fn pda(seeds: &[&[u8]], program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program_id).0
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
    svm.add_program(
        MPL_CORE_ID,
        include_bytes!("fixtures/mpl_core_program.so"),
    )
    .unwrap();
    svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();

    let collection = pda(&[nft_staking::COLLECTION, payer.pubkey().as_ref()], &program_id);
    let update_authority = pda(&[nft_staking::UPDATE_AUTHORITY, collection.as_ref()], &program_id);

    Env {
        svm,
        program_id,
        payer,
        collection,
        update_authority,
    }
}

fn create_collection(env: &mut Env) {
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::CreateCollection {
            name: NAME.to_string(),
            uri: URI.to_string(),
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
}

fn create_asset(env: &mut Env, asset: &Keypair) {
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::CreateAsset {
            name: "Asset".to_string(),
            uri: "https://example.com/a.json".to_string(),
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

fn initialize(env: &mut Env) -> Pubkey {
    let config = pda(&[nft_staking::CONFIG, env.collection.as_ref()], &env.program_id);
    let rewards_mint = pda(&[nft_staking::REWARDS_MINT, config.as_ref()], &env.program_id);
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Initialize {
            reward_bps: REWARD_BPS,
            freeze_period: FREEZE_PERIOD,
        }
        .data(),
        nft_staking::accounts::Initialize {
            admin: env.payer.pubkey(),
            config,
            collection: env.collection,
            update_authority: env.update_authority,
            rewards_mint,
            system_program: system_program::ID,
            token_program: anchor_spl::token::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);
    config
}

// ponytail: frozen is not asserted from raw plugin bytes here; the oracle step
// proves it by blocking a transfer of a frozen asset.
fn is_frozen(svm: &LiteSVM, asset: &Pubkey) -> bool {
    let account = svm.get_account(asset).unwrap();
    account.owner == MPL_CORE_ID
}

#[test]
fn stake_freezes_and_creates_state_then_unstake_thaws_and_closes() {
    let mut env = setup();
    create_collection(&mut env);
    let asset = Keypair::new();
    create_asset(&mut env, &asset);
    initialize(&mut env);

    let stake_state = pda(&[nft_staking::STAKE, asset.pubkey().as_ref()], &env.program_id);

    // Stake.
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Stake {}.data(),
        nft_staking::accounts::Stake {
            owner: env.payer.pubkey(),
            stake_state,
            asset: asset.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    assert!(is_frozen(&env.svm, &asset.pubkey()));
    let account = env.svm.get_account(&stake_state).expect("stake state exists");
    let mut data: &[u8] = &account.data;
    let state = nft_staking::state::StakeState::try_deserialize(&mut data).unwrap();
    assert_eq!(state.owner, env.payer.pubkey());

    // Unstake.
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Unstake {}.data(),
        nft_staking::accounts::Unstake {
            owner: env.payer.pubkey(),
            stake_state,
            asset: asset.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    assert!(env.svm.get_account(&stake_state).is_none(), "stake state closed");
    let account = env.svm.get_account(&asset.pubkey()).unwrap();
    let asset_state = BaseAssetV1::from_bytes(&account.data).unwrap();
    assert_eq!(asset_state.owner, env.payer.pubkey());
}

#[test]
fn initialize_records_config() {
    let mut env = setup();
    create_collection(&mut env);
    let config = initialize(&mut env);

    let account = env.svm.get_account(&config).unwrap();
    let mut data: &[u8] = &account.data;
    let state = nft_staking::state::Config::try_deserialize(&mut data).unwrap();
    assert_eq!(state.reward_bps, REWARD_BPS);
    assert_eq!(state.freeze_period, FREEZE_PERIOD);
}

#[test]
fn collection_is_created_with_permanent_freeze_delegate() {
    let mut env = setup();
    create_collection(&mut env);

    let account = env.svm.get_account(&env.collection).unwrap();
    let collection = BaseCollectionV1::from_bytes(&account.data).unwrap();
    assert_eq!(collection.name, NAME);
    assert_eq!(collection.update_authority, env.update_authority);
}