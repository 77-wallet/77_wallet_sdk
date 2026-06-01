use crate::harness::{ensure_worker_env, next_unique_id};
use alloy::primitives::U256;
use serial_test::serial;
use wallet_api::testkit::collect::shadow_collect_check_fee;
use wallet_database::{
    SqliteContext, entities::api_collect::ApiCollectStatus,
    repositories::api_wallet::collect::ApiCollectRepo,
};

use super::support::{
    build_eth_shadow_collect_worker, build_shadow_collect_worker, install_collect_eth_test_adapter,
    install_collect_test_adapter, install_collect_test_adapter_fee_shortage, seed_collect_order,
    seed_eth_collect_order,
};

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_fails_on_uninitialized_recipient() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(true, 27_309_206);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_sol_rent_fail_{}", next_unique_id());
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let err = shadow_collect_check_fee(&worker, &req)
        .await
        .expect_err("uninitialized SOL recipient should fail fee check");
    let msg = err.to_string();
    assert!(msg.contains("recipient account is not initialized"));
    assert!(msg.contains("rent-exempt minimum"));

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failure");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_allows_initialized_recipient() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter(false, 27_309_206);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_sol_rent_ok_{}", next_unique_id());
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("initialized SOL recipient should pass fee check");
    assert!(pass);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after success");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_with_partial_oracle_fallback() {
    let env = ensure_worker_env().await;
    let _guard =
        install_collect_eth_test_adapter(U256::from(100_000_000_000_000_000u128), 0.00000368);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_eth_partial_oracle_{}", next_unique_id());
    let req = seed_eth_collect_order(
        &collect_pool,
        &trade_no,
        "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
        "0xFCa230313618af2a33fa00455D8A5d1466C91332",
        "0.000015",
    )
    .await;

    let worker = build_eth_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("ETH collect fee check should succeed with partial oracle fallback");
    assert!(pass);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after success");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_fails_on_insufficient_balance() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_eth_test_adapter(U256::from(10_000_000_000_000u128), 0.00000368);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_eth_insufficient_{}", next_unique_id());
    let req = seed_eth_collect_order(
        &collect_pool,
        &trade_no,
        "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
        "0xFCa230313618af2a33fa00455D8A5d1466C91332",
        "0.000015",
    )
    .await;

    let worker = build_eth_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("ETH collect fee check should return a boolean result");
    assert!(!pass, "insufficient ETH balance should fail the fee check");

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failure");
    assert_eq!(persisted.status, ApiCollectStatus::Init);
}

#[serial]
#[tokio::test]
async fn collect_build_fee_estimation_shortage_reopens_fee_cycle() {
    let env = ensure_worker_env().await;
    let _guard = install_collect_test_adapter_fee_shortage(false, 0);

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let trade_no = format!("T_collect_fee_shortage_{}", next_unique_id());
    let req = seed_collect_order(
        &collect_pool,
        &trade_no,
        "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
    )
    .await;

    let worker = build_shadow_collect_worker(env).await;
    let pass = shadow_collect_check_fee(&worker, &req)
        .await
        .expect("fee estimate shortage should be downgraded to a boolean result");
    assert!(!pass, "fee estimate shortage must reopen the service-fee cycle instead of erroring");

    let affected = worker
        .invalidate_build_attempt_after_fee_check_failure(&req)
        .await
        .expect("fee shortage should reopen the fee cycle");
    assert_eq!(affected, 1);

    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after fee shortage reopen");
    assert_eq!(persisted.need_service_fee, Some(true));
    assert!(persisted.service_fee_uploaded_at.is_none());
    assert!(persisted.raw_tx.is_none());
    assert!(persisted.tx_hash.is_none());
}
