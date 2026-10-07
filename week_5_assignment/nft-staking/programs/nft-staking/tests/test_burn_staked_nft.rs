//! Burn-to-earn: burn the staked NFT for a one-time bonus.

mod common;

use {
    common::{Env, send_ok, token_amount},
    solana_awesome::{keypair::Keypair, signer::Signer},
};

const REWARD_BPS: u16 = 100;

#[test]
fn burn_staked_nft_destroys_asset_and_pays_bonus() {
    let mut env = Env::new();
    env.create_collection();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.initialize(REWARD_BPS);
    env.stake(&asset.pubkey());

    let ata = env.owner_rewards_ata();
    let stake_state = env.stake_state(&asset.pubkey());

    let ix = env.burn_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    assert!(env.asset_is_burned(&asset.pubkey()), "asset burned");
    assert!(env.svm.get_account(&stake_state).is_none(), "stake closed");
    assert_eq!(token_amount(&env.svm, &ata), nft_staking::BURN_BONUS);
}
