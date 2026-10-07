//! Burn-to-earn: burn the staked NFT for a one-time bonus.

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

    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::Initialize {
            reward_bps: 100,
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

#[test]
fn burn_staked_nft_destroys_asset_and_pays_bonus() {
    let mut env = setup();
    let ata = owner_ata(&env.rewards_mint, &env.payer.pubkey());

    let ix = Instruction::new_with_bytes(
        env.program_id,
        &nft_staking::instruction::BurnStakedNft {}.data(),
        nft_staking::accounts::BurnStakedNft {
            owner: env.payer.pubkey(),
            stake_state: env.stake_state,
            asset: env.asset.pubkey(),
            collection: env.collection,
            update_authority: env.update_authority,
            config: env.config,
            rewards_mint: env.rewards_mint,
            owner_rewards_ata: ata,
            mpl_core_program: MPL_CORE_ID,
            token_program: anchor_spl::token::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // mpl-core clears the asset discriminator on burn (data becomes a single
    // zero byte); the runtime reclaims the lamports at transaction end.
    let asset_account = env.svm.get_account(&env.asset.pubkey());
    let burned = match &asset_account {
        None => true,
        Some(a) => a.data.is_empty() || a.data == [0],
    };
    assert!(burned, "asset burned, got {:?}", asset_account.as_ref().map(|a| &a.data));
    // Stake state closed.
    assert!(env.svm.get_account(&env.stake_state).is_none(), "stake closed");
    // Bonus minted.
    let account = env.svm.get_account(&ata).expect("ata exists");
    let amount = u64::from_le_bytes(account.data[64..72].try_into().unwrap());
    assert_eq!(amount, nft_staking::BURN_BONUS);
}