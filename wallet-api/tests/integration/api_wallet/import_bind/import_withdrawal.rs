use std::time::Duration;

use crate::harness::{
    ApiWalletBackendCall, derive_uid, ensure_env, find_wallet_by_uid, load_wallet_by_uid, next_tag,
    open_api_wallet_pool, reset_fake, upsert_wallet_with_import_stage,
};
use serial_test::serial;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use super::support::{WALLET_PASSWORD, WITHDRAWAL_PHRASE};

#[tokio::test]
#[serial(import_bind)]
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
    for _ in 0..4 {
        env.fake_backend.enqueue_appid_uid_usage_used(true);
    }

    let recharge_uid = next_tag("uid-recharge");
    let recharge_address = upsert_wallet_with_import_stage(
        &env.db_dir,
        &env.sn,
        &recharge_uid,
        ApiWalletType::SubAccount,
        None,
        3,
    )
    .await;

    let withdrawal_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &next_tag("salt-waw"),
            &next_tag("withdraw-wallet"),
            WALLET_PASSWORD,
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
    assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("app-withdraw"));
    assert_eq!(recharge_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("app-withdraw"));

    env.fake_backend.with_calls(|calls| {
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdUidUsage(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
        assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
    });
}

#[tokio::test]
#[serial(import_bind)]
async fn import_withdrawal_wallet_recovers_incomplete_subaccount_then_completes() {
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    env.fake_backend.enqueue_query_uid_bind_info(
        "app-withdraw",
        "merchant-withdraw",
        true,
        &env.sn,
    );
    env.fake_backend.enqueue_query_uid_bind_info(
        "app-withdraw",
        "merchant-withdraw",
        true,
        &env.sn,
    );
    for _ in 0..4 {
        env.fake_backend.enqueue_appid_uid_usage_used(true);
    }

    let recharge_uid = next_tag("uid-recharge-partial");
    let recharge_address = upsert_wallet_with_import_stage(
        &env.db_dir,
        &env.sn,
        &recharge_uid,
        ApiWalletType::SubAccount,
        None,
        1,
    )
    .await;

    let withdrawal_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &next_tag("salt-waw-partial"),
            &next_tag("withdraw-wallet-partial"),
            WALLET_PASSWORD,
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect("withdrawal import should recover incomplete subaccount");

    let recharge_wallet = load_wallet_by_uid(env, &recharge_uid).await;
    let withdrawal_wallet = load_wallet_by_uid(env, &withdrawal_uid).await;
    assert_eq!(recharge_wallet.import_stage, 3);
    assert_eq!(withdrawal_wallet.import_stage, 3);
    assert_eq!(
        withdrawal_wallet.binding_address.as_deref(),
        Some(recharge_wallet.address.as_str())
    );
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("app-withdraw"));
    assert_eq!(recharge_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("app-withdraw"));
    assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
}

#[tokio::test]
#[serial(import_bind)]
async fn import_withdrawal_wallet_reimport_keeps_completion_and_account_count_stable() {
    let env = ensure_env().await;
    reset_fake(env);

    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    for _ in 0..14 {
        env.fake_backend.enqueue_appid_uid_usage_used(true);
    }
    for _ in 0..6 {
        env.fake_backend.enqueue_query_uid_bind_info(
            "app-withdraw",
            "merchant-withdraw",
            true,
            &env.sn,
        );
    }

    let recharge_uid = next_tag("uid-recharge-reimport");
    let recharge_address = upsert_wallet_with_import_stage(
        &env.db_dir,
        &env.sn,
        &recharge_uid,
        ApiWalletType::SubAccount,
        None,
        3,
    )
    .await;

    let withdrawal_salt = next_tag("salt-waw-reimport");
    let withdrawal_wallet_name = next_tag("withdraw-wallet-reimport");

    let first_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &withdrawal_salt,
            &withdrawal_wallet_name,
            WALLET_PASSWORD,
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect("first withdrawal import");

    let second_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &withdrawal_salt,
            &withdrawal_wallet_name,
            WALLET_PASSWORD,
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect("second withdrawal import");

    assert_eq!(first_uid, second_uid);

    let wallet = load_wallet_by_uid(env, &first_uid).await;
    assert_eq!(wallet.import_stage, 3);

    let pool = open_api_wallet_pool(&env.db_dir).await;
    let queried = wallet_database::repositories::api_wallet::wallet::ApiWalletRepo::find_by_uid(
        &pool, &first_uid,
    )
    .await
    .expect("query wallet by uid");
    assert_eq!(queried.map(|w| w.import_stage), Some(3));
}

#[tokio::test]
#[serial(import_bind)]
async fn import_withdrawal_wallet_with_concurrent_asset_reads_succeeds() {
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
    for _ in 0..4 {
        env.fake_backend.enqueue_appid_uid_usage_used(true);
    }
    env.fake_backend.set_appid_import_delay(Some(Duration::from_millis(80)));

    let recharge_uid = next_tag("uid-recharge-concurrent");
    let recharge_address = upsert_wallet_with_import_stage(
        &env.db_dir,
        &env.sn,
        &recharge_uid,
        ApiWalletType::SubAccount,
        None,
        3,
    )
    .await;

    let salt = next_tag("salt-waw-concurrent");
    let wallet_name = next_tag("withdraw-wallet-concurrent");
    let manager = &env.manager;
    let query_address = recharge_address.clone();
    let read_task = tokio::spawn(async move {
        let mut ok_count = 0;
        for _ in 0..12 {
            let res = manager.get_api_wallet_assets(Some(&query_address), None, None).await;
            if res.is_ok() {
                ok_count += 1;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        ok_count
    });

    let withdrawal_uid = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &salt,
            &wallet_name,
            WALLET_PASSWORD,
            None,
            ApiWalletType::Withdrawal,
            Some(&recharge_address),
        )
        .await
        .expect("import withdrawal wallet under concurrent reads");

    let ok_reads = read_task.await.expect("asset read task");
    assert!(ok_reads > 0, "expected concurrent read task to observe successful reads");

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
    assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
    assert_eq!(withdrawal_wallet.app_id.as_deref(), Some("app-withdraw"));
    assert_eq!(recharge_wallet.merchant_id.as_deref(), Some("merchant-withdraw"));
    assert_eq!(recharge_wallet.app_id.as_deref(), Some("app-withdraw"));

    env.fake_backend.set_appid_import_delay(None);
}

#[tokio::test]
#[serial(import_bind)]
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
    let recharge_address = upsert_wallet_with_import_stage(
        &env.db_dir,
        &env.sn,
        &recharge_uid,
        ApiWalletType::SubAccount,
        None,
        3,
    )
    .await;

    let salt = next_tag("salt-waw-usage-false");
    let expected_uid = derive_uid(WITHDRAWAL_PHRASE, &salt);
    let err = env
        .manager
        .import_api_wallet(
            1,
            WITHDRAWAL_PHRASE,
            &salt,
            &next_tag("withdraw-wallet-usage-false"),
            WALLET_PASSWORD,
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
