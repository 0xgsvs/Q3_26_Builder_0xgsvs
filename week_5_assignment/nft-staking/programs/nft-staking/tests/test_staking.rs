//! Base staking lifecycle: create collection -> create asset -> initialize ->
//! stake (freezes) -> unstake (thaws, closes stake state).

mod common;

use {
    anchor_lang::AccountDeserialize,
    common::{Env, NAME, read_total_staked},
    mpl_core::accounts::BaseCollectionV1,
    solana_awesome::{keypair::Keypair, signer::Signer},
};

const REWARD_BPS: u16 = 100;

#[test]
fn stake_freezes_and_creates_state_then_unstake_thaws_and_closes() {
    let mut env = Env::new();
    env.create_collection();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.initialize(REWARD_BPS);

    let stake_state = env.stake_state(&asset.pubkey());
    env.stake(&asset.pubkey());

    assert_eq!(env.stake_state_owner(&asset.pubkey()), env.payer.pubkey());

    env.unstake(&asset.pubkey());

    assert!(
        env.svm.get_account(&stake_state).is_none(),
        "stake state closed"
    );
    assert_eq!(env.asset_owner(&asset.pubkey()), env.payer.pubkey());
}

#[test]
fn initialize_records_config() {
    let mut env = Env::new();
    env.create_collection();
    env.initialize(REWARD_BPS);

    let account = env.svm.get_account(&env.config).unwrap();
    let mut data: &[u8] = &account.data;
    let state = nft_staking::state::Config::try_deserialize(&mut data).unwrap();
    assert_eq!(state.reward_bps, REWARD_BPS);
}

#[test]
fn collection_is_created_with_permanent_freeze_delegate() {
    let mut env = Env::new();
    env.create_collection();

    let account = env.svm.get_account(&env.collection).unwrap();
    let collection = BaseCollectionV1::from_bytes(&account.data).unwrap();
    assert_eq!(collection.name, NAME);
    assert_eq!(collection.update_authority, env.update_authority);
}

#[test]
fn total_staked_attribute_tracks_stake_and_unstake() {
    let mut env = Env::new();
    env.create_collection();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.initialize(REWARD_BPS);

    assert_eq!(
        read_total_staked(&env.svm, &env.collection),
        0,
        "fresh collection"
    );

    env.stake(&asset.pubkey());
    assert_eq!(
        read_total_staked(&env.svm, &env.collection),
        1,
        "after stake"
    );

    env.unstake(&asset.pubkey());
    assert_eq!(
        read_total_staked(&env.svm, &env.collection),
        0,
        "after unstake"
    );
}
