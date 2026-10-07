//! Oracle external plugin adapter: a time-based transfer gate.
//!
//! An `Oracle` PDA per collection stores the `Transfer` lifecycle validation
//! result (Approved inside the UTC open window, Rejected outside). A
//! permissionless crank updates it. The collection carries an Oracle external
//! plugin adapter with a reject-only capability, so mpl-core blocks asset
//! transfers while the oracle says Rejected.

mod common;

use {
    common::{Env, send_err, warp},
    mpl_core::{
        accounts::{BaseAssetV1, BaseCollectionV1},
        fetch_wrapped_external_plugin_adapter,
        types::{
            ExternalPluginAdapter, ExternalPluginAdapterKey, HookableLifecycleEvent,
            ValidationResultsOffset,
        },
    },
    solana_awesome::{keypair::Keypair, signer::Signer},
};

/// A unix timestamp whose UTC hour is `hour` (0..24).
fn ts_at_hour(hour: i64) -> i64 {
    1_700_000_000 / 86_400 * 86_400 + hour * 3_600
}

fn setup() -> Env {
    let mut env = Env::new();
    env.create_collection();
    env
}

#[test]
fn init_oracle_creates_account_with_anchor_offset_validation() {
    let mut env = setup();
    warp(&mut env.svm, 1_700_000_000); // arbitrary base time

    env.init_oracle();

    // mpl-core reads the 5 OracleValidation bytes at Anchor offset (8):
    // variant V1 = 1, then create/transfer/burn/update results. The oracle is
    // created Rejected (closed) so nothing slips through before the crank.
    assert_eq!(env.oracle_validation_bytes(), [1, 2, 1, 2, 2]);

    let oracle = env.read_oracle();
    assert_eq!(oracle.variant, 1);
    assert_eq!(oracle.transfer, 1, "created Rejected");
}

#[test]
fn crank_rejects_outside_open_hours() {
    let mut env = setup();
    env.init_oracle();

    // 03:00 UTC is outside the 09:00-17:00 window.
    warp(&mut env.svm, ts_at_hour(3));
    env.update_oracle();

    let oracle = env.read_oracle();
    assert_eq!(oracle.transfer, 1, "Rejected outside hours");
    assert_eq!(env.oracle_validation_bytes()[2], 1);
}

#[test]
fn crank_approves_inside_open_hours() {
    let mut env = setup();
    env.init_oracle();

    // 12:00 UTC is inside the window; 17:00 exactly is outside (half-open).
    warp(&mut env.svm, ts_at_hour(12));
    env.update_oracle();
    assert_eq!(env.read_oracle().transfer, 0, "Approved at 12:00");
    assert_eq!(env.oracle_validation_bytes()[2], 0);

    warp(&mut env.svm, ts_at_hour(17));
    env.update_oracle();
    assert_eq!(env.read_oracle().transfer, 1, "Rejected at 17:00");
}

#[test]
fn reward_only_near_boundary() {
    let mut env = setup();
    env.init_oracle();
    env.fund_vault(10_000_000);

    let crank = Keypair::new().pubkey();
    env.svm.airdrop(&crank, 1_000_000).unwrap();
    let before = env.svm.get_account(&crank).unwrap().lamports;

    // 12:00, far from any boundary: state updates but no reward.
    warp(&mut env.svm, ts_at_hour(12));
    env.update_oracle_as(&crank);
    assert_eq!(env.read_oracle().transfer, 0);
    assert_eq!(
        env.svm.get_account(&crank).unwrap().lamports,
        before,
        "no reward away from a boundary"
    );

    // 09:00:00 (the open boundary): reward is paid.
    warp(&mut env.svm, ts_at_hour(9));
    env.update_oracle_as(&crank);
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
    assert_eq!(oracle_plugin.base_address, env.oracle());
    assert_eq!(
        oracle_plugin.results_offset,
        ValidationResultsOffset::Anchor
    );

    // Build an AccountInfo to read the adapter's lifecycle checks from the
    // registry record.
    let mut lamports = account.lamports;
    let mut data = account.data.to_vec();
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
    let key = ExternalPluginAdapterKey::Oracle(env.oracle());
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
    env.create_asset(&asset);
    env.init_oracle();

    // Open the gate.
    warp(&mut env.svm, ts_at_hour(12));
    env.update_oracle();

    let recipient = Keypair::new();
    env.svm.airdrop(&recipient.pubkey(), 1_000_000).unwrap();
    env.transfer_asset(&asset.pubkey(), &recipient.pubkey());

    let account = env.svm.get_account(&asset.pubkey()).unwrap();
    let asset_state = BaseAssetV1::from_bytes(&account.data).unwrap();
    assert_eq!(asset_state.owner, recipient.pubkey(), "asset transferred");
}

#[test]
fn transfer_blocked_outside_open_hours() {
    let mut env = setup();
    let asset = Keypair::new();
    env.create_asset(&asset);
    env.init_oracle();

    // 03:00 UTC: outside the window, the oracle rejects.
    warp(&mut env.svm, ts_at_hour(3));
    env.update_oracle();
    assert_eq!(env.read_oracle().transfer, 1);

    let recipient = Keypair::new();
    env.svm.airdrop(&recipient.pubkey(), 1_000_000).unwrap();
    let ix = env.transfer_asset_ix(&asset.pubkey(), &recipient.pubkey());
    let failed = send_err(&mut env.svm, &[&env.payer], &[ix]);

    // mpl-core rejects the lifecycle: error 9 (InvalidAuthority).
    let err = format!("{:?}", failed.err);
    assert!(
        err.contains("Custom(9)"),
        "expected mpl-core reject, got {err}"
    );
    assert!(
        failed.meta.pretty_logs().contains("Reject"),
        "expected a Reject log, got:\n{}",
        failed.meta.pretty_logs()
    );

    let account = env.svm.get_account(&asset.pubkey()).unwrap();
    let asset_state = BaseAssetV1::from_bytes(&account.data).unwrap();
    assert_eq!(
        asset_state.owner,
        env.payer.pubkey(),
        "still owned by payer"
    );
}
