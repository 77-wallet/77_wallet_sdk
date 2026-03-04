#![cfg(feature = "integration-tests")]

// These smoke tests must run serially because wallet-api relies on a global OnceCell CONTEXT.
mod common;

use common::{
    ApiWalletBackendCall, ExpectedBindReq, assert_bind_call_once, derive_uid, ensure_env,
    find_wallet_by_uid, load_wallet_by_uid, next_tag, prepare_wallet_pair, reset_fake,
    snapshot_bind_fields, upsert_wallet,
};
use serial_test::serial;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

const SUBACCOUNT_PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const WITHDRAWAL_PHRASE: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";

#[tokio::test]
#[serial]
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
            "q1111111",
            None,
            ApiWalletType::SubAccount,
            None,
        )
        .await
        .expect("import subaccount wallet");

    let wallet = load_wallet_by_uid(env, &uid).await;
    assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::SubAccount as u8);
    assert_eq!(wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(wallet.merchant_id, "");
    assert_eq!(wallet.app_id.as_deref(), Some(""));

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial]
async fn import_withdrawal_wallet_ok_requires_binding_address() {
    // Scenario: import a withdrawal wallet bound to an existing sub-account wallet.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    env.fake_backend.enqueue_query_uid_bind_info(
        "app-withdraw",
        "merchant-withdraw",
        false,
        &env.sn,
    );
    env.fake_backend.enqueue_query_uid_bind_info(
        "app-withdraw",
        "merchant-withdraw",
        false,
        &env.sn,
    );
    env.fake_backend.enqueue_appid_uid_usage_used(true);

    let recharge_uid = next_tag("uid-recharge");
    let recharge_address =
        upsert_wallet(&env.db_dir, &env.sn, &recharge_uid, ApiWalletType::SubAccount, None).await;

    let withdrawal_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &next_tag("salt-waw"),
            &next_tag("withdraw-wallet"),
            "q1111111",
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect("import withdrawal wallet");

    let withdrawal_wallet = load_wallet_by_uid(env, &withdrawal_uid).await;
    let recharge_wallet = load_wallet_by_uid(env, &recharge_uid).await;
    assert_eq!(withdrawal_wallet.api_wallet_type as u8, ApiWalletType::Withdrawal as u8);
    assert_eq!(
        withdrawal_wallet.binding_address.as_deref(),
        Some(recharge_wallet.address.as_str())
    );
    assert_eq!(
        recharge_wallet.binding_address.as_deref(),
        Some(withdrawal_wallet.address.as_str())
    );
    assert_eq!(withdrawal_wallet.merchant_id, "merchant-withdraw");
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("app-withdraw"));
    assert_eq!(recharge_wallet.merchant_id, "merchant-withdraw");
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("app-withdraw"));

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdUidUsage(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial]
async fn scan_bind_ok_calls_backend_and_persists_bind_sn_and_relation() {
    // Scenario: scan_bind succeeds, backend is called once, and wallet relation fields persist.
    let env = ensure_env().await;
    reset_fake(env);
    let pair = prepare_wallet_pair(env).await;

    env.manager
        .scan_bind("scan-app-id", "scan-merchant-id", &pair.recharge_uid, &pair.withdrawal_uid)
        .await
        .expect("scan bind should succeed");

    let recharge_wallet = load_wallet_by_uid(env, &pair.recharge_uid).await;
    let withdrawal_wallet = load_wallet_by_uid(env, &pair.withdrawal_uid).await;
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("scan-app-id"));
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("scan-app-id"));
    assert_eq!(recharge_wallet.merchant_id, "scan-merchant-id");
    assert_eq!(withdrawal_wallet.merchant_id, "scan-merchant-id");
    assert_eq!(recharge_wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(withdrawal_wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(recharge_wallet.binding_address.as_deref(), Some(pair.withdrawal_address.as_str()));
    assert_eq!(withdrawal_wallet.binding_address.as_deref(), Some(pair.recharge_address.as_str()));

    assert_bind_call_once(
        &env.fake_backend,
        ExpectedBindReq {
            recharge_uid: pair.recharge_uid,
            withdrawal_uid: pair.withdrawal_uid,
            org_app_id: "scan-app-id".to_string(),
            sn: env.sn.clone(),
        },
    );
}

#[tokio::test]
#[serial]
async fn import_bind_ok_calls_appid_import_and_persists_bind_sn_and_relation() {
    // Scenario: import_bind succeeds, appid_import is invoked once, and bind fields persist.
    let env = ensure_env().await;
    reset_fake(env);
    let pair = prepare_wallet_pair(env).await;

    env.manager
        .import_bind(
            env.sn.as_str(),
            "import-bind-merchant",
            "import-bind-app",
            &pair.recharge_uid,
            &pair.withdrawal_uid,
        )
        .await
        .expect("import bind should succeed");

    let recharge_wallet = load_wallet_by_uid(env, &pair.recharge_uid).await;
    let withdrawal_wallet = load_wallet_by_uid(env, &pair.withdrawal_uid).await;
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("import-bind-app"));
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("import-bind-app"));
    assert_eq!(recharge_wallet.merchant_id, "import-bind-merchant");
    assert_eq!(withdrawal_wallet.merchant_id, "import-bind-merchant");
    assert_eq!(recharge_wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(withdrawal_wallet.sn.as_deref(), Some(env.sn.as_str()));
    assert_eq!(recharge_wallet.binding_address.as_deref(), Some(pair.withdrawal_address.as_str()));
    assert_eq!(withdrawal_wallet.binding_address.as_deref(), Some(pair.recharge_address.as_str()));

    env.fake_backend.with_calls(|calls| {
        let appid_import_calls: Vec<_> = calls
            .iter()
            .filter_map(|call| match call {
                ApiWalletBackendCall::AppIdImport(req) => Some(req),
                _ => None,
            })
            .collect();
        assert_eq!(appid_import_calls.len(), 1);
        let req = appid_import_calls[0];
        assert_eq!(req.sn, env.sn);
        assert_eq!(req.recharge_uid.as_deref(), Some(pair.recharge_uid.as_str()));
        assert_eq!(req.withdrawal_uid.as_deref(), Some(pair.withdrawal_uid.as_str()));
    });
}

#[tokio::test]
#[serial]
async fn import_bind_backend_fail_does_not_persist_relation() {
    // Scenario: import_bind backend failure must not mutate local relation fields.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.set_appid_import_error(Some("import bind backend fail"));
    let pair = prepare_wallet_pair(env).await;

    let before_recharge = snapshot_bind_fields(&load_wallet_by_uid(env, &pair.recharge_uid).await);
    let before_withdrawal =
        snapshot_bind_fields(&load_wallet_by_uid(env, &pair.withdrawal_uid).await);

    let err = env
        .manager
        .import_bind(
            env.sn.as_str(),
            "import-fail-merchant",
            "import-fail-app",
            &pair.recharge_uid,
            &pair.withdrawal_uid,
        )
        .await
        .expect_err("import_bind should fail when backend import fails");
    assert!(err.to_string().contains("import bind backend fail"));

    let after_recharge = snapshot_bind_fields(&load_wallet_by_uid(env, &pair.recharge_uid).await);
    let after_withdrawal =
        snapshot_bind_fields(&load_wallet_by_uid(env, &pair.withdrawal_uid).await);
    assert_eq!(after_recharge, before_recharge);
    assert_eq!(after_withdrawal, before_withdrawal);

    env.fake_backend.with_calls(|calls| {
        let appid_import_calls = calls
            .iter()
            .filter(|call| matches!(call, ApiWalletBackendCall::AppIdImport(_)))
            .count();
        assert_eq!(appid_import_calls, 1, "backend appid_import should be called exactly once");
    });
}

#[tokio::test]
#[serial]
async fn scan_bind_backend_fail_does_not_persist_bind() {
    // Scenario: scan_bind backend failure returns error and does not mutate local bind fields.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.set_wallet_bind_appid_error(Some("scan bind backend fail"));
    let pair = prepare_wallet_pair(env).await;

    let before_recharge = load_wallet_by_uid(env, &pair.recharge_uid).await;
    let before_withdrawal = load_wallet_by_uid(env, &pair.withdrawal_uid).await;

    let err = env
        .manager
        .scan_bind("scan-fail-app", "scan-fail-merchant", &pair.recharge_uid, &pair.withdrawal_uid)
        .await
        .expect_err("scan bind should fail");
    assert!(err.to_string().contains("scan bind backend fail"));

    let after_recharge = load_wallet_by_uid(env, &pair.recharge_uid).await;
    let after_withdrawal = load_wallet_by_uid(env, &pair.withdrawal_uid).await;
    assert_eq!(snapshot_bind_fields(&after_recharge), snapshot_bind_fields(&before_recharge));
    assert_eq!(snapshot_bind_fields(&after_withdrawal), snapshot_bind_fields(&before_withdrawal));

    env.fake_backend.with_calls(|calls| {
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0], ApiWalletBackendCall::WalletBindAppId(_)));
    });
}

#[tokio::test]
#[serial]
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
            "q1111111",
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

#[tokio::test]
#[serial]
async fn import_withdrawal_wallet_uid_usage_false_rejected_without_persist() {
    // Scenario: importing withdrawal wallet fails when backend says uid was never used by app id.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    env.fake_backend.enqueue_query_uid_bind_info(
        "app-usage-check",
        "merchant-usage-check",
        true,
        &env.sn,
    );
    env.fake_backend.enqueue_appid_uid_usage_used(false);

    let recharge_uid = next_tag("uid-recharge-usage-check");
    let recharge_address =
        upsert_wallet(&env.db_dir, &env.sn, &recharge_uid, ApiWalletType::SubAccount, None).await;

    let salt = next_tag("salt-waw-usage-false");
    let expected_uid = derive_uid(WITHDRAWAL_PHRASE, &salt);
    let err = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &salt,
            &next_tag("withdraw-wallet-usage-false"),
            "q1111111",
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect_err("withdrawal import must fail when appid_uid_usage returns false");
    let (code, _msg): (i64, String) = err.into();
    assert_eq!(code, 20004, "unexpected error code for withdrawal uid usage check");
    assert!(
        find_wallet_by_uid(env, &expected_uid).await.is_none(),
        "withdrawal record should not persist on appid usage check failure"
    );

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. })));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdUidUsage(_))));
        assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
        assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial]
async fn import_bind_missing_wallet_returns_not_found_and_no_backend_call() {
    // Scenario: import_bind returns not found when either wallet uid is missing and must not call backend.
    let env = ensure_env().await;
    reset_fake(env);

    let recharge_uid = next_tag("import-bind-only-recharge");
    upsert_wallet(&env.db_dir, &env.sn, &recharge_uid, ApiWalletType::SubAccount, None).await;

    let recharge_before = load_wallet_by_uid(env, &recharge_uid).await;
    let err = env
        .manager
        .import_bind(
            env.sn.as_str(),
            "missing-merchant",
            "missing-app",
            &recharge_uid,
            "missing-withdrawal-uid",
        )
        .await
        .expect_err("import_bind should fail when wallet does not exist");
    let (code, _msg): (i64, String) = err.into();
    assert_eq!(code, 20001, "unexpected error code for missing uid");

    let recharge_after = load_wallet_by_uid(env, &recharge_uid).await;
    assert_eq!(
        snapshot_bind_fields(&recharge_after),
        snapshot_bind_fields(&recharge_before),
        "existing wallet fields should remain unchanged"
    );

    env.fake_backend.with_calls(|calls| {
        assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdImport(_))));
    });
}

#[tokio::test]
#[serial]
async fn scan_bind_remote_first_then_persist() {
    // Scenario: remote bind must happen before local persistence, proven by no DB mutation on remote failure.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.set_wallet_bind_appid_error(Some("remote bind failed first"));
    let pair = prepare_wallet_pair(env).await;

    let before_recharge = snapshot_bind_fields(&load_wallet_by_uid(env, &pair.recharge_uid).await);
    let before_withdrawal =
        snapshot_bind_fields(&load_wallet_by_uid(env, &pair.withdrawal_uid).await);

    let err = env
        .manager
        .scan_bind(
            "orchestration-app",
            "orchestration-merchant",
            &pair.recharge_uid,
            &pair.withdrawal_uid,
        )
        .await
        .expect_err("scan_bind should fail when backend bind fails");
    assert!(err.to_string().contains("remote bind failed first"));

    let after_recharge = snapshot_bind_fields(&load_wallet_by_uid(env, &pair.recharge_uid).await);
    let after_withdrawal =
        snapshot_bind_fields(&load_wallet_by_uid(env, &pair.withdrawal_uid).await);
    assert_eq!(after_recharge, before_recharge);
    assert_eq!(after_withdrawal, before_withdrawal);
}
