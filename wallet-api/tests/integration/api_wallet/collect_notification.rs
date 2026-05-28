use std::path::Path;

use crate::harness::{
    SMOKE_WALLET_PASSWORD, ensure_worker_env, next_unique_id, open_api_wallet_pool,
};
use serial_test::serial;
use wallet_api::messaging::notify::FrontendNotifyEvent;
use wallet_database::{
    SqliteContext,
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
    repositories::api_wallet::{collect::ApiCollectRepo, wallet::ApiWalletRepo},
};

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

#[serial]
#[tokio::test]
async fn collect_notification_retry_on_existing_trade_no() {
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let uid = format!("uid_collect_notify_{}", next_unique_id());
    let trade_no = format!("T_collect_notify_retry_{}", next_unique_id());
    let from_addr = format!("from-collect-notify-{}", next_unique_id());
    let to_addr = format!("to-collect-notify-{}", next_unique_id());
    let _wallet_addr =
        seed_wallet(&env.db_dir, &uid, "collect-notify-wallet", ApiWalletType::SubAccount).await;

    let (fail_tx, fail_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    drop(fail_rx);
    env._manager
        .set_frontend_notify_sender(fail_tx)
        .await
        .expect("install failing frontend sender");

    let first = env
        ._manager
        .api_collect_order(
            &from_addr, &to_addr, "12.34", "digest", "sol", None, "USDC", &trade_no, 2, &uid,
        )
        .await;
    assert!(first.is_err(), "frontend notify failure should bubble up");

    let collect_pool_ctx =
        SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
            .await
            .expect("open api transaction sqlite");
    let collect_pool = collect_pool_ctx.into_transaction_db_pool().expect("transaction pool");
    let persisted = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after failed notify");
    assert_eq!(persisted.status, ApiCollectStatus::Init);

    let (ok_tx, mut ok_rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    env._manager.set_frontend_notify_sender(ok_tx).await.expect("install working frontend sender");

    env._manager
        .api_collect_order(
            &from_addr, &to_addr, "12.34", "digest", "sol", None, "USDC", &trade_no, 2, &uid,
        )
        .await
        .expect("retrying the same collect order should resend frontend notify");

    let notify = tokio::time::timeout(std::time::Duration::from_secs(1), ok_rx.recv())
        .await
        .expect("timed out waiting for collect notify")
        .expect("missing collect notify event");
    let notify_json = serde_json::to_value(&notify).expect("serialize collect notify");
    assert_eq!(notify_json["event"], "COLLECT");
    assert_eq!(notify_json["data"]["uid"], uid);
    assert_eq!(notify_json["data"]["fromAddr"], from_addr);
    assert_eq!(notify_json["data"]["toAddr"], to_addr);
    assert_eq!(notify_json["data"]["value"], "12.34");
}
