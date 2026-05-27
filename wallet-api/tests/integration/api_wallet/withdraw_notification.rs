use std::path::Path;

use crate::harness::{
    SMOKE_WALLET_PASSWORD, decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id,
    open_api_wallet_pool,
};
use serial_test::serial;
use wallet_api::{
    messaging::notify::FrontendNotifyEvent,
    test::withdraw::{
        scan_withdraw_intent_labels_once, send_tx_ack_via_worker as send_withdraw_tx_ack_via_worker,
    },
};
use wallet_database::{
    SqliteContext,
    entities::{
        api_trade_type::ApiTradeType, api_wallet::ApiWalletType, api_withdraw::ApiWithdrawStatus,
    },
    repositories::api_wallet::{wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo},
};

use crate::harness::worker::CapturedHttpRequest;

const TEST_SN: &str = "collect-worker-test-sn";

async fn seed_wallet(
    db_dir: &Path,
    uid: &str,
    wallet_name: &str,
    wallet_type: ApiWalletType,
) -> String {
    let pool = open_api_wallet_pool(db_dir).await;
    let address = format!("0xwallet{:016x}", next_unique_id());
    let seed_enc = wallet_api::test::seed::encrypt_seed(SMOKE_WALLET_PASSWORD, b"seed").await;
    ApiWalletRepo::upsert(
        &pool,
        uid,
        wallet_name,
        &address,
        b"phrase",
        &seed_enc,
        wallet_type,
        None,
        TEST_SN,
        0,
    )
    .await
    .expect("seed wallet");
    address
}

fn count_withdraw_tx_ack_requests(requests: &[CapturedHttpRequest], trade_no: &str) -> usize {
    requests
        .iter()
        .filter(|req| {
            req.path
                .contains(wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK)
        })
        .filter(|req| {
            let payload = decrypt_captured_api_backend_body(&req.body);
            payload["tradeNo"].as_str() == Some(trade_no)
                && payload["ackType"].as_str() == Some("TX")
                && payload["type"].as_str() == Some("WD")
        })
        .count()
}

async fn wait_for_withdraw_tx_ack_count(trade_no: &str) -> usize {
    let env = ensure_worker_env().await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let requests = env.recorder.snapshot();
        let count = count_withdraw_tx_ack_requests(&requests, trade_no);
        if count > 0 || std::time::Instant::now() >= deadline {
            return count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[serial]
#[tokio::test]
async fn withdraw_notification_retry_on_existing_trade_no() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_withdraw_notify_{}", next_unique_id());
    let trade_no = format!("T_withdraw_notify_retry_{}", next_unique_id());
    let from_addr = format!("from-withdraw-retry-{}", next_unique_id());
    let to_addr = format!("to-withdraw-retry-{}", next_unique_id());
    let _wallet_addr =
        seed_wallet(&env.db_dir, &uid, "withdraw-notify-wallet", ApiWalletType::Withdrawal).await;

    let (fail_tx, fail_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    drop(fail_rx);
    env._manager
        .set_frontend_notify_sender(fail_tx)
        .await
        .expect("install failing frontend sender");

    let first = env
        ._manager
        .api_withdrawal_order(
            &from_addr, &to_addr, "56.78", "digest", "sol", None, "USDC", &trade_no, 1, &uid,
        )
        .await;
    assert!(first.is_err(), "frontend notify failure should bubble up");

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let persisted =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw after failed notify");
    assert_eq!(persisted.init_status, ApiWithdrawStatus::AuditPass);
    assert_eq!(persisted.status, ApiWithdrawStatus::InitOrder);

    let (ok_tx, mut ok_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    env._manager.set_frontend_notify_sender(ok_tx).await.expect("install working frontend sender");

    env._manager
        .api_withdrawal_order(
            &from_addr, &to_addr, "56.78", "digest", "sol", None, "USDC", &trade_no, 1, &uid,
        )
        .await
        .expect("retrying the same withdraw order should resend frontend notify");

    let notify = tokio::time::timeout(std::time::Duration::from_secs(1), ok_rx.recv())
        .await
        .expect("timed out waiting for withdraw notify")
        .expect("missing withdraw notify event");
    let notify_json = serde_json::to_value(&notify).expect("serialize withdraw notify");
    assert_eq!(notify_json["event"], "WITHDRAW");
    assert_eq!(notify_json["data"]["uid"], uid);
    assert_eq!(notify_json["data"]["fromAddr"], from_addr);
    assert_eq!(notify_json["data"]["toAddr"], to_addr);
    assert_eq!(notify_json["data"]["value"], "56.78");

    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    send_withdraw_tx_ack_via_worker(tx_pool.clone(), core_pool, &trade_no)
        .await
        .expect("send withdraw tx ack after retry");

    let tx_ack_request_count = wait_for_withdraw_tx_ack_count(&trade_no).await;

    assert_eq!(
        tx_ack_request_count, 1,
        "retrying the same withdraw order should still emit only one TX ack request"
    );
}

#[serial]
#[tokio::test]
async fn withdraw_tx_ack_template_sends_once_and_persists_fact() {
    // Arrange
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_withdraw_ack_{}", next_unique_id());
    let trade_no = format!("T_withdraw_ack_{}", next_unique_id());
    let from_addr = format!("from-withdraw-ack-{}", next_unique_id());
    let to_addr = format!("to-withdraw-ack-{}", next_unique_id());

    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    let tx_pool = tx_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        &uid,
        "withdraw",
        &from_addr,
        &to_addr,
        "56.78",
        "digest",
        "sol",
        None,
        "USDC",
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

    // Act
    send_withdraw_tx_ack_via_worker(tx_pool.clone(), core_pool.clone(), &trade_no)
        .await
        .expect("send withdraw tx ack");

    // Assert: backend side effect and DB fact
    let tx_ack_request_count = wait_for_withdraw_tx_ack_count(&trade_no).await;
    assert_eq!(tx_ack_request_count, 1, "withdraw order should emit exactly one TX ack request");

    let persisted =
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(&tx_pool, &trade_no, ApiTradeType::Withdraw)
            .await
            .expect("load withdraw after tx ack");
    assert!(persisted.tx_ack_sent_at.is_some(), "successful tx ack should persist tx_ack_sent_at");

    let labels =
        scan_withdraw_intent_labels_once(tx_pool.clone()).await.expect("scan withdraw intents");
    assert!(
        labels.iter().all(|label| label != "SendTxAck"),
        "withdraw with tx_ack_sent_at must not re-enter SendTxAck; labels: {labels:?}"
    );

    // Assert: repeated act stays idempotent
    send_withdraw_tx_ack_via_worker(tx_pool.clone(), core_pool, &trade_no)
        .await
        .expect("repeat withdraw tx ack should be idempotent");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let requests = env.recorder.snapshot();
    let tx_ack_request_count = count_withdraw_tx_ack_requests(&requests, &trade_no);
    assert_eq!(tx_ack_request_count, 1, "withdraw order should not emit a second TX ack request");
}
