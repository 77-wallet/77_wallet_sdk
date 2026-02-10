use super::{collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain};
use sqlx;
use tempfile::TempDir;
use wallet_database::{
    SqliteContext,
    entities::{
        api_collect::ApiCollectStatus, api_fee::ApiFeeStatus, api_trade_type::ApiTradeType,
        api_withdraw::ApiWithdrawStatus,
    },
    repositories::api_wallet::{
        collect::ApiCollectRepo, fee::ApiFeeRepo, withdraw::ApiWithdrawRepo,
    },
};

struct TestFundsDb {
    _dir: TempDir,
    pool: wallet_database::CollectDbPool,
}

impl TestFundsDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_funds.db"))
            .await
            .expect("init api_funds.db");
        let pool = ctx.into_collect_db_pool().expect("collect pool");
        Self { _dir: dir, pool }
    }
}

#[tokio::test]
async fn collect_repeat_should_still_write_transaction_time() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_repeat_tx_time";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "name",
        "from",
        "to",
        "1",
        "validate",
        "tron",
        None,
        "USDT",
        trade_no,
        2,
        ApiCollectStatus::Success,
        0,
    )
    .await
    .expect("insert collect");

    let before =
        ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no).await.expect("get collect");
    assert!(before.transaction_time.is_none(), "precondition: transaction_time should be NULL");

    ApiCollectDomain::confirm_tx_with_pool(&db.pool, trade_no, true, 0)
        .await
        .expect("confirm collect");

    let after = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("get collect after");
    assert!(after.transaction_time.is_some(), "transaction_time should be written");
}

#[tokio::test]
async fn collect_not_found_should_error() {
    let db = TestFundsDb::new().await;
    let res = ApiCollectDomain::confirm_tx_with_pool(&db.pool, "T_collect_missing", true, 0).await;
    assert!(res.is_err(), "missing trade_no must error to avoid ACK");
}

#[tokio::test]
async fn fee_repeat_should_still_write_transaction_time() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_fee_repeat_tx_time";

    ApiFeeRepo::upsert_api_fee(
        &db.pool, "uid", "name", "from", "to", "1", "validate", "tron", None, "USDT", trade_no, 3,
    )
    .await
    .expect("insert fee");

    // 强制设置为 Success 且 transaction_time 为空（复现“repeat early return 不补事实”场景）
    sqlx::query(
        r#"
        UPDATE api_fee
        SET
            status = $2,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind(ApiFeeStatus::Success)
    .execute(db.pool.as_ref())
    .await
    .expect("set fee status");

    let before = ApiFeeRepo::get_api_fee_by_trade_no(&db.pool, trade_no).await.expect("get fee");
    assert!(before.transaction_time.is_none(), "precondition: transaction_time should be NULL");

    ApiFeeDomain::confirm_tx_with_pool(&db.pool, trade_no, true).await.expect("confirm fee");

    let after =
        ApiFeeRepo::get_api_fee_by_trade_no(&db.pool, trade_no).await.expect("get fee after");
    assert!(after.transaction_time.is_some(), "transaction_time should be written");
}

#[tokio::test]
async fn fee_not_found_should_error() {
    let db = TestFundsDb::new().await;
    let res = ApiFeeDomain::confirm_tx_with_pool(&db.pool, "T_fee_missing", true).await;
    assert!(res.is_err(), "missing trade_no must error to avoid ACK");
}

#[tokio::test]
async fn withdraw_repeat_should_still_write_transaction_time() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_withdraw_repeat_tx_time";

    ApiWithdrawRepo::upsert_api_withdraw(
        &db.pool,
        "uid",
        "name",
        "from",
        "to",
        "1",
        "validate",
        "tron",
        None,
        "USDT",
        trade_no,
        ApiTradeType::Withdraw,
        0,
        None,
        ApiWithdrawStatus::Success,
        ApiWithdrawStatus::Success,
        "0",
        "0",
        None,
        None,
    )
    .await
    .expect("insert withdraw");

    let before =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&db.pool, trade_no, ApiTradeType::Withdraw)
            .await
            .expect("get withdraw");
    assert!(before.transaction_time.is_none(), "precondition: transaction_time should be NULL");

    let _outcome = ApiWithdrawDomain::confirm_tx_with_pool(&db.pool, trade_no, true)
        .await
        .expect("confirm withdraw");

    let after =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&db.pool, trade_no, ApiTradeType::Withdraw)
            .await
            .expect("get withdraw after");
    assert!(after.transaction_time.is_some(), "transaction_time should be written");
}

#[tokio::test]
async fn withdraw_not_found_should_error() {
    let db = TestFundsDb::new().await;
    let res = ApiWithdrawDomain::confirm_tx_with_pool(&db.pool, "T_withdraw_missing", true).await;
    assert!(res.is_err(), "missing trade_no must error to avoid ACK");
}
