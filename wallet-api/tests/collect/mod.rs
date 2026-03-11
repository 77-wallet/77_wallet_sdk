use chrono::Utc;
use serial_test::serial;
use sqlx;
use std::{
    collections::VecDeque,
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::OnceCell,
};
use wallet_api::{
    ApiWalletBackend,
    dirs::Dirs,
    manager::WalletManager,
    test::collect::{
        build_collect_tx_exec_receipt_payload, upload_collect_tx_exec_receipt_via_backend,
        upload_collect_tx_exec_receipt_via_worker,
    },
};
use wallet_database::{
    ApiWalletDbPool, SqliteContext,
    entities::api_collect::{ApiCollectEntity, ApiCollectStatus},
    repositories::api_wallet::collect::ApiCollectRepo,
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::{
    request::{
        KeysInitReq,
        api_wallet::wallet::{
            AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq,
        },
    },
    response_vo::api_wallet::wallet::{AppIdUidUsageRes, KeysUidCheckRes, QueryUidBindInfoRes},
};

const TEST_SN: &str = "collect-worker-test-sn";
const TEST_DEVICE_TYPE: &str = "ANDROID";
const TEST_PUB_KEY: &str = r#"-----BEGIN PUBLIC KEY-----
MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEWDZNP0ClbeWJey9hBr2rsjSayQEBywnv
ZXi0RberQCAp+06fOjvr+jZI5qwYGglmMkGJw49tbni6qgm4QNV6WQ==
-----END PUBLIC KEY-----"#;
static WORKER_ENV: OnceCell<WorkerTestEnv> = OnceCell::const_new();
static UNIQUE_ID: AtomicU64 = AtomicU64::new(1);

struct TestFundsDb {
    _dir: TempDir,
    pool: wallet_database::ApiFundsDbPool,
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

#[derive(Clone, Debug)]
struct CapturedHttpRequest {
    path: String,
    body: String,
}

#[derive(Default)]
struct MockBackendState {
    requests: VecDeque<CapturedHttpRequest>,
}

#[derive(Clone, Default)]
struct MockBackendRecorder {
    state: Arc<Mutex<MockBackendState>>,
}

impl MockBackendRecorder {
    fn push(&self, req: CapturedHttpRequest) {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.push_back(req);
    }

    fn pop(&self) -> Option<CapturedHttpRequest> {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.pop_front()
    }

    fn reset(&self) {
        let mut state = self.state.lock().expect("mock backend lock poisoned");
        state.requests.clear();
    }
}

async fn pop_request_with_retry(recorder: &MockBackendRecorder) -> Option<CapturedHttpRequest> {
    for _ in 0..20 {
        if let Some(req) = recorder.pop() {
            return Some(req);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    None
}

#[derive(Default)]
struct NoopApiWalletBackend;

#[async_trait::async_trait]
impl ApiWalletBackend for NoopApiWalletBackend {
    async fn wallet_bind_appid(
        &self,
        _req: BindAppIdReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn init_api_wallet(
        &self,
        _req: AppIdImportReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn old_keys_init(
        &self,
        _req: KeysInitReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn appid_import(
        &self,
        _req: AppIdImportReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn appid_import_recharge_wallet(
        &self,
        _req: AppIdImportRechargeWalletReq,
    ) -> Result<(), wallet_api::error::service::ServiceError> {
        Ok(())
    }

    async fn keys_uid_check(
        &self,
        _uid: &str,
    ) -> Result<KeysUidCheckRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }

    async fn query_uid_bind_info(
        &self,
        _uid: &str,
    ) -> Result<QueryUidBindInfoRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }

    async fn appid_uid_usage(
        &self,
        _req: AppIdUidUsageReq,
    ) -> Result<AppIdUidUsageRes, wallet_api::error::service::ServiceError> {
        Err(wallet_api::error::service::ServiceError::System(
            wallet_api::error::system::SystemError::Internal("noop".to_string()),
        ))
    }
}

struct WorkerTestEnv {
    _manager: WalletManager,
    backend_url: String,
    db_dir: PathBuf,
    recorder: MockBackendRecorder,
}

async fn start_mock_backend_server() -> io::Result<(String, MockBackendRecorder)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let recorder = MockBackendRecorder::default();
    let recorder_clone = recorder.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let recorder = recorder_clone.clone();
            tokio::spawn(async move {
                let mut header_buf = Vec::new();
                let mut temp = [0u8; 1024];
                let header_end;
                loop {
                    let n = match stream.read(&mut temp).await {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    header_buf.extend_from_slice(&temp[..n]);
                    if let Some(pos) = header_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = pos + 4;
                        break;
                    }
                }

                let header_text = String::from_utf8_lossy(&header_buf[..header_end]);
                let path = header_text
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or_default()
                    .to_string();
                let content_length = header_text
                    .lines()
                    .find_map(|line| {
                        let lower = line.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);

                let mut body = header_buf[header_end..].to_vec();
                while body.len() < content_length {
                    let n = match stream.read(&mut temp).await {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    body.extend_from_slice(&temp[..n]);
                }

                recorder.push(CapturedHttpRequest {
                    path,
                    body: String::from_utf8_lossy(&body).to_string(),
                });

                let response_body = r#"{"success":true,"code":"200","msg":"ok","data":null}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    Ok((format!("http://{}", addr), recorder))
}

fn create_test_root_dir() -> PathBuf {
    let id = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("wallet_api_collect_worker_{id}"));
    std::fs::create_dir_all(&root).expect("create test root");
    root
}

async fn open_api_wallet_pool(db_dir: &Path) -> ApiWalletDbPool {
    let sqlite = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_wallet.db"))
        .await
        .expect("open api wallet sqlite");
    let pool = sqlite.get_pool().expect("api wallet db pool");
    ApiWalletDbPool::new(pool)
}

async fn ensure_worker_env() -> &'static WorkerTestEnv {
    WORKER_ENV
        .get_or_init(|| async {
            let (backend_url, recorder) =
                start_mock_backend_server().await.expect("start mock backend server");
            // Match wallet-api test env setup and disable system proxy resolution for reqwest.
            unsafe {
                std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
            }
            let config = wallet_api::config::Config::new(&format!(
                r#"
app_code: "test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "{backend_url}"
  test_url: "{backend_url}"
  prod_url: "{backend_url}"
aggregate_api:
  dev_url: "{backend_url}"
  test_url: "{backend_url}"
  prod_url: "{backend_url}"
oss:
  access_key_id: "id"
  access_key_secret: "secret"
  bucket_name: "bucket"
  endpoint: "oss-endpoint"
"#
            ))
            .expect("parse test config");

            let root = create_test_root_dir();
            let dirs = Dirs::new(root.to_str().expect("utf8 root dir")).expect("create dirs");
            GLOBAL_KEY.set_shared_secret(TEST_PUB_KEY).expect("set shared secret");
            let manager = WalletManager::new_for_test(
                TEST_SN,
                TEST_DEVICE_TYPE,
                config,
                dirs.clone(),
                Arc::new(NoopApiWalletBackend),
            )
            .await
            .expect("create wallet manager");

            WorkerTestEnv { _manager: manager, backend_url, db_dir: dirs.db_dir.clone(), recorder }
        })
        .await
}

async fn current_backend_url() -> Option<String> {
    let app_state = wallet_api::app_state::APP_STATE.read().await;
    app_state.url().backend.clone()
}

fn decrypt_captured_api_backend_body(body: &str) -> serde_json::Value {
    #[derive(serde::Deserialize)]
    struct CapturedApiBackendBody {
        key: String,
        data: String,
    }
    #[derive(serde::Deserialize)]
    struct CapturedApiBackendRequest {
        body: CapturedApiBackendBody,
    }

    let req: CapturedApiBackendRequest =
        serde_json::from_str(body).expect("deserialize captured backend request");
    let key = wallet_utils::base64_to_bytes(&req.body.key).expect("decode encrypted key");
    let data = wallet_utils::base64_to_bytes(&req.body.data).expect("decode encrypted data");
    let plain = GLOBAL_KEY.decrypt(&data, &key).expect("decrypt backend body");
    serde_json::from_slice(&plain).expect("deserialize decrypted payload")
}

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_blockhash_rebuild_refresh";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    let invalidated = ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");
    assert_eq!(invalidated, 1);

    let after_invalidate = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load collect after invalidate");
    assert!(after_invalidate.raw_tx.is_none(), "stale raw_tx must be cleared");
    assert!(after_invalidate.tx_hash.is_none(), "stale tx_hash must be cleared");
    assert_eq!(
        after_invalidate.to_addr, "old-to",
        "rebuild invalidation must not invent a new execution address on its own"
    );

    ApiCollectRepo::update_api_collect_to_addr(&db.pool, trade_no, "new-to")
        .await
        .expect("persist rebuilt to_addr");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    assert!(rebuilt.raw_tx.is_none(), "rebuild starts from cleared build facts");
    assert!(rebuilt.tx_hash.is_none(), "rebuild starts from cleared tx hash");
    assert_eq!(
        rebuilt.to_addr, "new-to",
        "next build must persist the latest strategy address before generating new raw_tx"
    );
}

fn base_collect_for_receipt() -> ApiCollectEntity {
    ApiCollectEntity {
        id: 1,
        name: "collect".to_string(),
        uid: "uid".to_string(),
        from_addr: "from".to_string(),
        to_addr: "persisted-to".to_string(),
        value: "1.12".to_string(),
        validate: "digest".to_string(),
        chain_code: "sol".to_string(),
        token_addr: Some("token".to_string()),
        symbol: "USDC".to_string(),
        trade_no: "trade-no".to_string(),
        trade_type: 2,
        risk_addr: 1,
        status: ApiCollectStatus::SendingTx,
        nonce: 0,
        tx_hash: Some("hash".to_string()),
        transaction_fee: "0".to_string(),
        transaction_time: Some(Utc::now()),
        block_height: "0".to_string(),
        notes: String::new(),
        post_tx_count: 0,
        post_confirm_tx_count: 0,
        err_code: None,
        err_msg: String::new(),
        order_ack_attempted_at: None,
        order_ack_sent_at: Some(Utc::now()),
        raw_tx: Some("{}".to_string()),
        resource_consume: "0".to_string(),
        building_at: None,
        last_broadcast_at: Some(Utc::now()),
        broadcast_uncertain_since_at: None,
        broadcast_uncertain_retry_count: 0,
        broadcast_uncertain_last_checked_at: None,
        broadcast_uncertain_reconciled_at: None,
        broadcast_uncertain_rebroadcast_count: 0,
        result_ack_attempted_at: None,
        result_ack_sent_at: None,
        result_ack_send_count: 0,
        tx_res_received_at: None,
        service_fee_attempted_at: None,
        service_fee_uploaded_at: None,
        need_service_fee: None,
        ever_needed_service_fee: false,
        tx_fee_res_ack_sent_at: None,
        tx_exec_receipt_attempted_at: None,
        tx_exec_receipt_uploaded_at: None,
        finished_at: None,
        created_at: Utc::now(),
        updated_at: Some(Utc::now()),
    }
}

#[tokio::test]
async fn collect_tx_exec_receipt_uses_persisted_to_addr() {
    let req = base_collect_for_receipt();

    let payload = build_collect_tx_exec_receipt_payload(&req, &req.trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "persisted-to");
    assert_eq!(payload_json["hash"], "hash");
}

#[tokio::test]
async fn collect_rebuild_then_receipt_upload_uses_rebuilt_to_addr() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_rebuild_then_receipt";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("new-to")
    .bind("new-hash")
    .bind("{\"rebuilt\":true}")
    .execute(db.pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    let payload = build_collect_tx_exec_receipt_payload(&rebuilt, trade_no);
    let payload_json = serde_json::to_value(&payload).expect("serialize payload");

    assert_eq!(payload_json["to"], "new-to");
    assert_eq!(payload_json["hash"], "new-hash");
}

#[serial]
#[tokio::test]
async fn collect_side_effect_worker_marks_tx_exec_receipt_uploaded_after_rebuild() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let backend_url = current_backend_url().await.expect("backend url set in app state");
    assert_eq!(backend_url, env.backend_url, "worker should use the mock backend URL");

    let collect_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_funds.db"))
        .await
        .expect("open api funds sqlite");
    let collect_pool = collect_pool_ctx.into_collect_db_pool().expect("collect pool");
    let core_pool = open_api_wallet_pool(&env.db_dir).await;
    let trade_no =
        format!("T_collect_worker_receipt_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed));

    ApiCollectRepo::upsert_api_collect(
        &collect_pool,
        "uid",
        "collect",
        "from-worker",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        &trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(&trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(collect_pool.as_ref())
    .await
    .expect("set stale build facts");

    ApiCollectRepo::invalidate_raw_tx_for_rebuild(&collect_pool, &trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET to_addr = $2,
            tx_hash = $3,
            raw_tx = $4,
            last_broadcast_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(&trade_no)
    .bind("rebuilt-to")
    .bind("rebuilt-hash")
    .bind("{\"rebuilt\":true}")
    .execute(collect_pool.as_ref())
    .await
    .expect("persist rebuilt execution facts");

    upload_collect_tx_exec_receipt_via_worker(collect_pool.clone(), core_pool, &trade_no)
        .await
        .expect("upload tx exec receipt should succeed");

    let rec = ApiCollectRepo::get_api_collect_by_trade_no(&collect_pool, &trade_no)
        .await
        .expect("load collect after worker upload");
    assert!(rec.tx_exec_receipt_attempted_at.is_some(), "worker should mark attempted_at");
    assert!(
        rec.tx_exec_receipt_uploaded_at.is_some(),
        "worker should mark uploaded_at after successful backend upload"
    );

    let payload_json = serde_json::to_value(build_collect_tx_exec_receipt_payload(&rec, &trade_no))
        .expect("serialize rebuilt payload");
    assert_eq!(payload_json["tradeNo"], trade_no);
    assert_eq!(payload_json["to"], "rebuilt-to");
    assert_eq!(payload_json["hash"], "rebuilt-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}

#[serial]
#[tokio::test]
async fn collect_backend_api_direct_upload_hits_mock_server() {
    let env = ensure_worker_env().await;
    env.recorder.reset();
    let req = ApiCollectEntity {
        trade_no: format!("T_collect_direct_backend_{}", UNIQUE_ID.fetch_add(1, Ordering::Relaxed)),
        tx_hash: Some("direct-hash".to_string()),
        to_addr: "direct-to".to_string(),
        from_addr: "direct-from".to_string(),
        last_broadcast_at: Some(Utc::now()),
        ..base_collect_for_receipt()
    };

    upload_collect_tx_exec_receipt_via_backend(&req, &req.trade_no)
        .await
        .expect("direct backend upload should succeed");

    let captured = pop_request_with_retry(&env.recorder)
        .await
        .expect("captured backend request for direct upload");
    assert!(
        captured.path.contains("awallet/aw/trans/executeComplete"),
        "unexpected backend path: {}",
        captured.path
    );
    let payload_json = decrypt_captured_api_backend_body(&captured.body);
    assert_eq!(payload_json["tradeNo"], req.trade_no);
    assert_eq!(payload_json["to"], "direct-to");
    assert_eq!(payload_json["hash"], "direct-hash");
    assert_eq!(payload_json["status"], "SUCCESS");
}
