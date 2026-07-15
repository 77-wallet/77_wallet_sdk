use wallet_database::{
    ApiResourceDbPool, ApiTransactionDbPool,
    entities::{
        api_collect::ApiCollectStatus,
        api_resource_gate::{ApiResourceBlockReason, ApiResourceDependencyType},
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::collect::ApiCollectRepo,
};

pub(crate) async fn seed_blocked_collect(collect_pool: &ApiTransactionDbPool, trade_no: &str) {
    ApiCollectRepo::upsert_api_collect(
        collect_pool,
        "uid",
        "collect",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        trade_no,
        ApiTradeType::Collect as u8,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        "UPDATE api_collect SET order_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE trade_no = ?",
    )
    .bind(trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("seed order ack");

    ApiCollectRepo::mark_resource_blocked(
        collect_pool,
        trade_no,
        ApiResourceBlockReason::NeedPlatformDelegate,
        Some(&format!("rsc_delegate_{trade_no}")),
        Some(ApiResourceDependencyType::PlatformDelegate),
    )
    .await
    .expect("seed blocked collect");
}

pub(crate) async fn seed_resource_result(
    collect_pool: &ApiTransactionDbPool,
    resource_pool: &ApiResourceDbPool,
    trade_no: &str,
    resource_trade_no: &str,
    origin_trade_type: ApiTradeType,
    success: bool,
) {
    let (tx_status, err_code, err_msg, result_status, result_payload) = if success {
        ("success", None, None, 1_i64, r#"{"status":true}"#)
    } else {
        ("fail", Some("ERR_6008"), Some("delegate failed"), 2_i64, r#"{"status":false}"#)
    };

    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            tx_hash, tx_status, err_code, err_msg, result_status, result_received_at, result_payload,
            tx_res_received_at, created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'tx_hash_collect_resource', ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .bind(tx_status)
    .bind(err_code)
    .bind(err_msg)
    .bind(result_status)
    .bind(result_payload)
    .execute(resource_pool.as_ref())
    .await
    .expect("seed resource delegation row for result ack");

    if !success && origin_trade_type == ApiTradeType::Collect {
        ApiCollectRepo::mark_resource_blocked(
            collect_pool,
            trade_no,
            ApiResourceBlockReason::NeedLocalDelegate,
            None,
            Some(ApiResourceDependencyType::LocalDelegate),
        )
        .await
        .expect("seed collect local delegate fallback");
    }
}

pub(crate) async fn mark_collect_chain_success(
    collect_pool: &ApiTransactionDbPool,
    trade_no: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_collect
        SET transaction_time = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            tx_hash = 'collect_chain_success_hash',
            block_height = '12345',
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = ?
        "#,
    )
    .bind(trade_no)
    .execute(collect_pool.as_ref())
    .await
    .expect("mark collect chain success");
}

pub(crate) async fn seed_failed_resource_receipt_row(
    resource_pool: &ApiResourceDbPool,
    trade_no: Option<&str>,
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
    .bind(trade_no)
    .bind(ApiTradeType::Collect as i64)
    .bind(resource_trade_no)
    .execute(resource_pool.as_ref())
    .await
    .expect("seed failed collect delegation row");
}

pub(crate) async fn insert_failed_delegation_ready_for_receipt_scan(
    resource_pool: &ApiResourceDbPool,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            task_ack_sent_at, building_at, tx_status, err_code, err_msg,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, NULL, 2,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            'fail', 'ERR_6008', 'sdk internal error',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(resource_trade_no)
    .execute(resource_pool.as_ref())
    .await
    .expect("seed failed resource delegation row");
}
