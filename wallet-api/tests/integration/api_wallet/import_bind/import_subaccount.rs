use crate::harness::{
    ApiWalletBackendCall, derive_uid, ensure_env, find_wallet_by_uid, load_wallet_by_uid, next_tag,
    open_api_wallet_pool, reset_fake,
};
use serial_test::serial;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use super::support::{SUBACCOUNT_PHRASE, WALLET_PASSWORD};

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_ok_unbound() {
    // Scenario: import a sub-account wallet for an unbound UID and persist local wallet record.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
    env.fake_backend.enqueue_query_uid_bind_info("", "", false, &env.sn);

    let uid = env
        .manager
        .import_api_wallet(
            1,
            SUBACCOUNT_PHRASE,
            &next_tag("salt-sub"),
            &next_tag("sub-wallet"),
            WALLET_PASSWORD,
            None,
            ApiWalletType::SubAccount,
            None,
        )
        .await
        .expect("import subaccount wallet");

    let wallet = load_wallet_by_uid(env, &uid).await;
    assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::SubAccount as u8);
    assert_eq!(wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(wallet.merchant_id.as_deref(), Some(""));
    assert_eq!(wallet.app_id.as_deref(), Some(""));

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_query_failure_does_not_persist_half_state() {
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
    env.fake_backend.set_query_uid_bind_info_error(Some("bind-info timeout"));

    let salt = next_tag("salt-sub-fail");
    let wallet_name = next_tag("sub-wallet-fail");
    let uid = derive_uid(SUBACCOUNT_PHRASE, &salt);

    let err = env
        .manager
        .import_api_wallet(
            1,
            SUBACCOUNT_PHRASE,
            &salt,
            &wallet_name,
            WALLET_PASSWORD,
            None,
            ApiWalletType::SubAccount,
            None,
        )
        .await
        .expect_err("import should fail when bind info query fails");

    let err_msg = format!("{err:?}");
    assert!(err_msg.contains("bind-info timeout"));
    assert!(
        find_wallet_by_uid(env, &uid).await.is_none(),
        "wallet record should not be persisted when preflight query fails"
    );

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. })));
        assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_sets_progress_stage_before_completion() {
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
    env.fake_backend.enqueue_query_uid_bind_info("", "", false, &env.sn);

    let salt = next_tag("salt-sub-stage");
    let wallet_name = next_tag("sub-wallet-stage");
    let uid = derive_uid(SUBACCOUNT_PHRASE, &salt);

    let imported_uid = env
        .manager
        .import_api_wallet(
            1,
            SUBACCOUNT_PHRASE,
            &salt,
            &wallet_name,
            WALLET_PASSWORD,
            None,
            ApiWalletType::SubAccount,
            None,
        )
        .await
        .expect("import subaccount wallet");

    assert_eq!(imported_uid, uid);

    let wallet = load_wallet_by_uid(env, &uid).await;
    assert_eq!(wallet.import_stage, 3);

    let pool = open_api_wallet_pool(&env.db_dir).await;
    let queried =
        wallet_database::repositories::api_wallet::wallet::ApiWalletRepo::find_by_uid(&pool, &uid)
            .await
            .expect("query wallet by uid");
    assert_eq!(queried.map(|w| w.import_stage), Some(3));
}

#[tokio::test]
#[serial(import_bind)]
async fn import_subaccount_wallet_uid_status_mismatch_rejected_without_persist() {
    // Scenario: importing as SubAccount fails when backend reports a different uid status.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);

    let salt = next_tag("salt-sub-mismatch");
    let uid = derive_uid(SUBACCOUNT_PHRASE, &salt);
    let err = env
        .manager
        .import_api_wallet(
            1,
            SUBACCOUNT_PHRASE,
            &salt,
            &next_tag("sub-wallet-mismatch"),
            WALLET_PASSWORD,
            None,
            ApiWalletType::SubAccount,
            None,
        )
        .await
        .expect_err("subaccount import must fail on uid mismatch");
    let (code, _msg): (i64, String) = err.into();
    assert_eq!(code, 20002, "unexpected error code for uid status mismatch");
    assert!(
        find_wallet_by_uid(env, &uid).await.is_none(),
        "wallet record should not persist when uid type mismatches"
    );

    env.fake_backend.with_calls(|calls| {
        assert_eq!(calls.len(), 1, "only uid check should run");
        assert!(matches!(calls[0], ApiWalletBackendCall::KeysUidCheck { .. }));
    });
}
