use crate::harness::{
    ApiWalletBackendCall, ExpectedBindReq, assert_bind_call_once, ensure_env, load_wallet_by_uid,
    next_tag, prepare_wallet_pair, reset_fake, snapshot_bind_fields, upsert_wallet,
};
use serial_test::serial;
use wallet_database::entities::api_wallet::ApiWalletType;

#[tokio::test]
#[serial(import_bind)]
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
    assert_eq!(recharge_wallet.merchant_id.as_deref(), Some("scan-merchant-id"));
    assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some("scan-merchant-id"));
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
#[serial(import_bind)]
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
    assert_eq!(recharge_wallet.merchant_id.as_deref(), Some("import-bind-merchant"));
    assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some("import-bind-merchant"));
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
#[serial(import_bind)]
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
#[serial(import_bind)]
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
#[serial(import_bind)]
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
#[serial(import_bind)]
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
