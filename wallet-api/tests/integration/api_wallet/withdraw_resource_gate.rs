use serial_test::serial;
use sqlx;
use wallet_api::test_support::withdraw::{
    scan_withdraw_intent_labels_once,
    send_resource_result_ack_via_worker as send_withdraw_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker as upload_withdraw_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    SqliteContext,
    entities::{
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
        api_withdraw::ApiWithdrawStatus,
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::harness::{
    decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id, open_api_wallet_pool,
};

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_uses_wd_rsc_dl_type() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    let resource_trade_no = format!("RSC_WD_ACK_{}", next_unique_id());
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, 'W_ORIGIN_ACK', 1,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(&resource_trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed withdraw delegation row for result ack");

    send_withdraw_resource_result_ack_via_worker(tx_pool, core_pool, &resource_trade_no)
        .await
        .expect("send resource result ack");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let matched = loop {
        let requests = env.recorder.snapshot();
        let found = requests.iter().any(|req| {
            req.path
                .contains(wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK)
                && {
                    let payload = decrypt_captured_api_backend_body(&req.body);
                    payload["tradeNo"].as_str() == Some(resource_trade_no.as_str())
                        && payload["ackType"].as_str() == Some("TX_RES")
                        && payload["type"].as_str() == Some("WD_RSC_DL")
                }
        });
        if found || std::time::Instant::now() >= deadline {
            break found;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    let captured_requests = env.recorder.snapshot();
    let decoded_event_acks: Vec<_> = captured_requests
        .iter()
        .filter(|req| {
            req.path
                .contains(wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK)
        })
        .map(|req| decrypt_captured_api_backend_body(&req.body))
        .collect();
    assert!(
        matched,
        "withdraw resource result ack must use WD_RSC_DL; decoded event ack payloads: {:?}; captured requests: {:?}",
        decoded_event_acks, captured_requests
    );
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_releases_origin_withdraw_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    let trade_no = format!("W_RSC_RELEASE_{}", next_unique_id());
    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        &trade_no,
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
    .bind(format!("DL_W_{trade_no}"))
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(&trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed blocked withdraw");

    let resource_trade_no = format!("DL_W_{trade_no}");
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
            'tx_hash_withdraw_release', 'success', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(&trade_no)
    .bind(ApiTradeType::Withdraw as i64)
    .bind(&resource_trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed withdraw delegation row for result ack");

    send_withdraw_resource_result_ack_via_worker(tx_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send withdraw resource result ack");

    let withdraw =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw");
    assert!(withdraw.resource_gate_released_at.is_some());
    assert_eq!(
        withdraw.resource_gate_result,
        Some(ApiResourceGateResult::ResourceDelegationSuccess)
    );

    let labels =
        scan_withdraw_intent_labels_once(tx_pool.clone()).await.expect("scan withdraw labels");
    assert!(
        labels.iter().any(|label| label == "BuildTx"),
        "released withdraw should re-enter BuildTx"
    );
}

#[serial]
#[tokio::test]
async fn withdraw_failed_resource_bypass_reopens_withdraw_build_flow() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    let trade_no = format!("W_RSC_FAIL_{}", next_unique_id());
    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        &trade_no,
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
    .bind(format!("DL_W_{trade_no}"))
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(&trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed blocked withdraw");

    let resource_trade_no = format!("DL_W_{trade_no}");
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
    .bind(&trade_no)
    .bind(ApiTradeType::Withdraw as i64)
    .bind(&resource_trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed failed withdraw delegation row");

    let labels_before = scan_withdraw_intent_labels_once(tx_pool.clone())
        .await
        .expect("scan withdraw labels before bypass");
    assert!(
        labels_before.iter().all(|label| label != "BuildTx"),
        "blocked withdraw should not be eligible for BuildTx before failed delegation bypass"
    );

    upload_withdraw_resource_tx_exec_receipt_via_worker(
        tx_pool.clone(),
        core_pool,
        &resource_trade_no,
    )
    .await
    .expect("upload withdraw resource tx exec receipt");

    let withdraw =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw");
    assert!(withdraw.resource_gate_released_at.is_some());
    assert_eq!(
        withdraw.resource_gate_result,
        Some(ApiResourceGateResult::ResourceDelegationFailedBypass)
    );

    let labels_after = scan_withdraw_intent_labels_once(tx_pool.clone())
        .await
        .expect("scan withdraw labels after bypass");
    assert!(
        labels_after.iter().any(|label| label == "BuildTx"),
        "failed delegation bypass should reopen the withdraw build flow"
    );
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_without_origin_trade_no_does_not_release_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    let trade_no = format!("W_RSC_NO_ORIGIN_{}", next_unique_id());
    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        &trade_no,
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
    .bind(format!("DL_W_{trade_no}"))
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(&trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed blocked withdraw");

    let resource_trade_no = format!("DL_W_{trade_no}");
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            tx_hash, tx_status, result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, NULL, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'tx_hash_withdraw_no_origin', 'success', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(ApiTradeType::Withdraw as i64)
    .bind(&resource_trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed withdraw delegation row without origin trade");

    send_withdraw_resource_result_ack_via_worker(tx_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send withdraw resource result ack");

    let withdraw =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw");
    assert!(withdraw.resource_gate_released_at.is_none());
    assert!(withdraw.resource_gate_result.is_none());
}

#[serial]
#[tokio::test]
async fn withdraw_resource_result_ack_for_collect_origin_does_not_release_withdraw_gate() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    let trade_no = format!("W_RSC_WRONG_ORIGIN_{}", next_unique_id());
    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        &trade_no,
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
    .bind(format!("DL_W_{trade_no}"))
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(&trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed blocked withdraw");

    let resource_trade_no = format!("DL_W_{trade_no}");
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
            'tx_hash_withdraw_wrong_origin', 'success', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(&trade_no)
    .bind(ApiTradeType::Collect as i64)
    .bind(&resource_trade_no)
    .execute(tx_pool.as_ref())
    .await
    .expect("seed withdraw delegation row with collect origin type");

    send_withdraw_resource_result_ack_via_worker(tx_pool.clone(), core_pool, &resource_trade_no)
        .await
        .expect("send withdraw resource result ack");

    let withdraw =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw");
    assert!(withdraw.resource_gate_released_at.is_none());
    assert!(withdraw.resource_gate_result.is_none());
}
