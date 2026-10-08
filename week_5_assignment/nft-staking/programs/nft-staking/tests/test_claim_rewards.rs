//! Claim rewards without unstaking: time-based accrual, NFT stays frozen.

mod common;

use common::{Env, send_ok, token_amount, warp};
use mpl_core::accounts::BaseAssetV1;
use solana_awesome::{keypair::Keypair, signer::Signer};

const REWARD_BPS: u16 = 100;
const STAKE_AT: i64 = 1_000;

/// Collection + asset + config, then stake at `STAKE_AT`.
fn setup() -> (Env, Keypair) {
    setup_with_bps(REWARD_BPS)
}

fn setup_with_bps(reward_bps: u16) -> (Env, Keypair) {
    let mut env = Env::new();
    env.create_collection();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.initialize(reward_bps);

    warp(&mut env.svm, STAKE_AT);
    env.stake(&asset.pubkey());

    (env, asset)
}

#[test]
fn claim_mints_accrued_rewards_and_keeps_nft_staked() {
    let (mut env, asset) = setup();
    let ata = env.owner_rewards_ata();

    // 500 seconds at 100 bps/day of REWARD_UNIT.
    warp(&mut env.svm, STAKE_AT + 500);

    let ix = env.claim_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    let expected = 500u64 * REWARD_BPS as u64 * nft_staking::REWARD_UNIT
        / nft_staking::SECONDS_PER_PERIOD
        / nft_staking::BPS_DENOMINATOR;
    assert_eq!(token_amount(&env.svm, &ata), expected);
    assert_eq!(env.last_claim(&asset.pubkey()), STAKE_AT + 500);

    // Asset still exists and is owned by mpl-core (still staked, not burned).
    let asset_account = env.svm.get_account(&asset.pubkey()).unwrap();
    BaseAssetV1::from_bytes(&asset_account.data).unwrap();
}

#[test]
fn double_claim_does_not_double_pay_same_window() {
    let (mut env, asset) = setup();
    let ata = env.owner_rewards_ata();

    warp(&mut env.svm, STAKE_AT + 100); // 100s of accrual
    let ix = env.claim_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], std::slice::from_ref(&ix));
    send_ok(&mut env.svm, &[&env.payer], &[ix]);

    // Claimed twice in the same instant: only the first window pays.
    let expected = 100u64 * REWARD_BPS as u64 * nft_staking::REWARD_UNIT
        / nft_staking::SECONDS_PER_PERIOD
        / nft_staking::BPS_DENOMINATOR;
    assert_eq!(token_amount(&env.svm, &ata), expected);
}

/// Total minted over many short claims must equal one claim over the same span,
/// so the truncated fraction is carried instead of discarded.
#[test]
fn short_claims_carry_the_remainder() {
    const SHORT_BPS: u16 = 1;
    const STEPS: i64 = 24;
    const STEP_SECS: i64 = 500;
    let total_secs = STEPS * STEP_SECS;

    let expected = total_secs as u64 * SHORT_BPS as u64 * nft_staking::REWARD_UNIT
        / nft_staking::SECONDS_PER_PERIOD
        / nft_staking::BPS_DENOMINATOR;

    // One claim over the whole span.
    let (mut env, asset) = setup_with_bps(SHORT_BPS);
    let ata = env.owner_rewards_ata();
    warp(&mut env.svm, STAKE_AT + total_secs);
    let ix = env.claim_ix(&asset.pubkey());
    send_ok(&mut env.svm, &[&env.payer], &[ix]);
    assert_eq!(token_amount(&env.svm, &ata), expected, "single claim");

    // The same span claimed in many short windows.
    let (mut env, asset) = setup_with_bps(SHORT_BPS);
    let ata = env.owner_rewards_ata();
    for i in 1..=STEPS {
        warp(&mut env.svm, STAKE_AT + i * STEP_SECS);
        let ix = env.claim_ix(&asset.pubkey());
        send_ok(&mut env.svm, &[&env.payer], &[ix]);
    }
    assert_eq!(
        token_amount(&env.svm, &ata),
        expected,
        "many short claims must match the single claim"
    );
    // Subunit remainder stays below one full period's subunit total.
    let denominator = nft_staking::SECONDS_PER_PERIOD * nft_staking::BPS_DENOMINATOR;
    assert!(env.reward_carry(&asset.pubkey()) < denominator);
}
