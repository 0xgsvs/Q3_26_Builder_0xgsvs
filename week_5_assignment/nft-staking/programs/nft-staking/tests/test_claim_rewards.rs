//! Claim rewards without unstaking: time-based accrual, NFT stays frozen.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mpl_core::{accounts::BaseAssetV1, ID as MPL_CORE_ID},
    solana_awesome::{
        keypair::Keypair,
        message::{Message, VersionedMessage},
        signer::Signer,
        transaction::versioned::VersionedTransaction,
    },
    solana_clock::Clock,
};

const REWARD_BPS: u16 = 100;

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
    config: Pubkey,
    rewards_mint: Pubkey,
    asset: Keypair,
    stake_state: Pubkey,
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
    let config = pda(&[nft_staking::CONFIG, collection.as_ref()], &program_id);
    let rewards_mint = pda(&[nft_staking::REWARDS_MINT, config.as_ref()], &program_id);
    let asset = Keypair::new();
    let stake_state = pda(&[nft_staking::STAKE, asset.pubkey().as_ref()], &program_id);

    let mut env = Env {
        svm,
        program_id,
        payer,
        collection,
        update_authority,
        config,
        rewards_mint,
        asset,
        stake_state,
    };

    // create collection
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

    // create asset
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::CreateAsset {
            name: "A".into(),
            uri: "u".into(),
        }
        .data(),
        nft_staking::accounts::CreateAsset {
            payer: env.payer.pubkey(),
            asset: env.asset.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer, &env.asset], &[ix]);

    // initialize config
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Initialize {
            reward_bps: REWARD_BPS,
            freeze_period: 0,
        }
        .data(),
        nft_staking::accounts::Initialize {
            admin: env.payer.pubkey(),
            config: env.config,
            collection: env.collection,
            update_authority: env.update_authority,
            rewards_mint: env.rewards_mint,
            system_program: system_program::ID,
            token_program: anchor_spl::token::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // stake at t = 1000
    warp(&mut env.svm, 1_000);
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Stake {}.data(),
        nft_staking::accounts::Stake {
            owner: env.payer.pubkey(),
            stake_state: env.stake_state,
            asset: env.asset.pubkey(),
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

fn owner_ata(mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), anchor_spl::token::ID.as_ref(), mint.as_ref()],
        &anchor_spl::associated_token::ID,
    )
    .0
}

fn token_amount(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account = svm.get_account(ata).expect("ata exists");
    // SPL token account: amount is at offset 64, 8 bytes LE.
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

#[test]
fn claim_mints_accrued_rewards_and_keeps_nft_staked() {
    let mut env = setup();
    let ata = owner_ata(&env.rewards_mint, &env.payer.pubkey());

    // 500 seconds later: 500 * 100 = 50_000 reward base units.
    warp(&mut env.svm, 1_500);

    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::ClaimRewards {}.data(),
        nft_staking::accounts::ClaimRewards {
            owner: env.payer.pubkey(),
            stake_state: env.stake_state,
            asset: env.asset.pubkey(),
            config: env.config,
            collection: env.collection,
            rewards_mint: env.rewards_mint,
            owner_rewards_ata: ata,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    assert_eq!(token_amount(&env.svm, &ata), 500 * 100);

    // Stake state still exists (NFT stays staked).
    let account = env.svm.get_account(&env.stake_state).unwrap();
    let mut data: &[u8] = &account.data;
    let state = nft_staking::state::StakeState::try_deserialize(&mut data).unwrap();
    assert_eq!(state.last_claim, 1_500);

    // Asset still exists and is owned by mpl-core (still staked, not burned).
    let asset_account = env.svm.get_account(&env.asset.pubkey()).unwrap();
    BaseAssetV1::from_bytes(&asset_account.data).unwrap();
}

#[test]
fn double_claim_does_not_double_pay_same_window() {
    let mut env = setup();
    let ata = owner_ata(&env.rewards_mint, &env.payer.pubkey());

    warp(&mut env.svm, 1_100); // 100s -> 10_000
    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::ClaimRewards {}.data(),
        nft_staking::accounts::ClaimRewards {
            owner: env.payer.pubkey(),
            stake_state: env.stake_state,
            asset: env.asset.pubkey(),
            config: env.config,
            collection: env.collection,
            rewards_mint: env.rewards_mint,
            owner_rewards_ata: ata,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix.clone()]);
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // Claimed twice in the same instant: only the first window pays.
    assert_eq!(token_amount(&env.svm, &ata), 100 * 100);
}