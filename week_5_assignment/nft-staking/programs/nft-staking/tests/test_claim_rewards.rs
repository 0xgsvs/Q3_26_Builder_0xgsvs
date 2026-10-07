//! Claim rewards without unstaking: time-based accrual, NFT stays frozen.

mod common;

use {
    common::{Env, send_ok, token_amount, warp},
    mpl_core::accounts::BaseAssetV1,
    solana_awesome::{keypair::Keypair, signer::Signer},
};

const REWARD_BPS: u16 = 100;
const STAKE_AT: i64 = 1_000;

/// Collection + asset + config, then stake at `STAKE_AT`.
fn setup() -> (Env, Keypair) {
    let mut env = Env::new();
    env.create_collection();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.initialize(REWARD_BPS);

    warp(&mut env.svm, STAKE_AT);
    env.stake(&asset.pubkey());

    (env, asset)
}

#[test]
fn claim_mints_accrued_rewards_and_keeps_nft_staked() {
    let (mut env, asset) = setup();
    let ata = env.owner_rewards_ata();

    // 500 seconds later: 500 * 100 = 50_000 reward base units.
    warp(&mut env.svm, STAKE_AT + 500);

    let ix = env.claim_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    assert_eq!(token_amount(&env.svm, &ata), 500 * 100);
    assert_eq!(env.last_claim(&asset.pubkey()), STAKE_AT + 500);

    // Asset still exists and is owned by mpl-core (still staked, not burned).
    let asset_account = env.svm.get_account(&asset.pubkey()).unwrap();
    BaseAssetV1::from_bytes(&asset_account.data).unwrap();
}

#[test]
fn double_claim_does_not_double_pay_same_window() {
    let (mut env, asset) = setup();
    let ata = env.owner_rewards_ata();

    warp(&mut env.svm, STAKE_AT + 100); // 100s -> 10_000
    let ix = env.claim_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], std::slice::from_ref(&ix));
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // Claimed twice in the same instant: only the first window pays.
    assert_eq!(token_amount(&env.svm, &ata), 100 * 100);
}
