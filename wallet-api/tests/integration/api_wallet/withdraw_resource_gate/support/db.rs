use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_resource_gate::{ApiResourceBlockReason, ApiResourceDependencyType},
        api_trade_type::ApiTradeType,
        api_withdraw::ApiWithdrawStatus,
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::harness::worker::WorkerTestEnv;

pub(crate) async fn open_transaction_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    tx_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

pub(crate) async fn seed_withdraw(pool: &ApiTransactionDbPool, trade_no: &str) {
    ApiWithdrawRepo::upsert_api_withdraw(
        pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        trade_no,
        None,
        None,
        None,
        ApiTradeType::Withdraw,
        1,
        None,
        ApiWithdrawStatus::AuditPass,
        ApiWithdrawStatus::InitOrder,
        "",
        "",
        None,
        None,
    )
    .await
    .expect("insert withdraw");
}

pub(crate) async fn mark_withdraw_blocked(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_withdraws
        SET tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            audit_passed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            resource_block_reason = ?,
            resource_dependency_trade_no = ?,
            resource_dependency_type = ?
        WHERE trade_no = ?
        "#,
    )
    .bind(ApiResourceBlockReason::NeedPlatformDelegate.as_i64())
    .bind(resource_trade_no)
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed blocked withdraw");
}

pub(crate) async fn seed_resource_delegation_ready_for_ack(
    pool: &ApiTransactionDbPool,
    origin_trade_no: &str,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, 1,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(origin_trade_no)
    .bind(resource_trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed withdraw delegation row for result ack");
}

pub(crate) async fn seed_successful_resource_delegation(
    pool: &ApiTransactionDbPool,
    origin: Option<(&str, ApiTradeType)>,
    resource_trade_no: &str,
    tx_hash: &str,
) {
    let (origin_trade_no, origin_trade_type) = origin
        .map_or((None, ApiTradeType::Withdraw), |(trade_no, trade_type)| {
            (Some(trade_no), trade_type)
        });

    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            tx_hash, tx_status, result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            ?, 'success', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(origin_trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .bind(tx_hash)
    .execute(pool.as_ref())
    .await
    .expect("seed successful resource delegation row");
}

pub(crate) async fn seed_failed_resource_delegation(
    pool: &ApiTransactionDbPool,
    origin_trade_no: &str,
    origin_trade_type: ApiTradeType,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            err_code, err_msg, tx_status,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'delegate_failed', 'delegate failed', 'fail',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(origin_trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed failed withdraw delegation row");
}
