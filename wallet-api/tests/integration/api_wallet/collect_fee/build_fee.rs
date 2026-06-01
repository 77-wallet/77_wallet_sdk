use crate::harness::{ensure_worker_env, next_unique_id};
use serial_test::serial;
use wallet_api::testkit::collect::shadow_collect_check_fee;
use wallet_database::{SqliteContext, repositories::api_wallet::collect::ApiCollectRepo};

use super::support::{
    build_shadow_collect_worker, install_collect_test_adapter, seed_collect_order,
};

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_reopens_fee_cycle_on_first_insufficient_balance() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_fee_reopen_initial_{}", next_unique_id());
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee check should return a boolean result");
    assert!(!pass, "low balance should still fail the build fee check on the first attempt");

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("first insufficient balance should reopen fee cycle");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after reopen");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_none());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_preserves_completed_fee_cycle_facts() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_fee_reopen_rebuild_{}", next_unique_id());
    seed_collect_order(&collect_pool, &trade_no, "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW")
        .await;

    sqlx::query(
        r#"
        UPDATE api_collect
        SET service_fee_uploaded_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_fee_res_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            need_service_fee = false,
            ever_needed_service_fee = true,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(&trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed fee cycle facts");

    let req = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("reload collect with fee cycle facts");

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee check should return a boolean result");
    assert!(
        !pass,
        "low balance should still fail the build fee check when fee facts already exist"
    );

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("completed fee cycle should preserve fee facts during rebuild");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after rebuild-only invalidation");
    assert_eq!(persisted.need_service_fee, Some(false));
    assert!(persisted.service_fee_uploaded_at.is_some());
    assert!(persisted.tx_fee_res_ack_sent_at.is_some());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}
