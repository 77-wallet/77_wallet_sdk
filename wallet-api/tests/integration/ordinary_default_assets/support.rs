use crate::harness::worker::{MockBackendRecorder, MockResponseGate, start_mock_backend_server};
use std::sync::Arc;
use wallet_api::{
    Context,
    testkit::{chain_sync, mqtt},
};
use wallet_database::CoreDbPool;

pub(super) struct OrdinaryDefaultAssetsScenario {
    ctx: &'static Context,
    pool: CoreDbPool,
    recorder: MockBackendRecorder,
    _dir: tempfile::TempDir,
    api_asset_count: i64,
    events: std::sync::Mutex<
        tokio::sync::mpsc::UnboundedReceiver<wallet_api::messaging::notify::FrontendNotifyEvent>,
    >,
}
impl OrdinaryDefaultAssetsScenario {
    pub async fn new() -> anyhow::Result<Self> {
        let (url, recorder) = start_mock_backend_server()?;
        unsafe {
            std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
        }
        let config = wallet_api::config::Config::new(&format!(
            r#"
app_code: "test"
crypto: {{ aes_key: "1234567890abcdef", aes_iv: "abcdef1234567890" }}
backend_api: {{ dev_url: "{url}", test_url: "{url}", prod_url: "{url}" }}
aggregate_api: {{ dev_url: "{url}", test_url: "{url}", prod_url: "{url}" }}
oss: {{ access_key_id: "test", access_key_secret: "test", bucket_name: "test", endpoint: "test" }}
"#
        ))?;
        let dir = tempfile::tempdir()?;
        let ctx = Box::leak(Box::new(
            Context::new_without_handles_for_test(
                wallet_api::dirs::Dirs::new(dir.path().to_str().unwrap())?,
                config,
            )
            .await?,
        ));
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        ctx.set_frontend_notify_sender(Some(sender)).await?;
        let pool = chain_sync::core_pool(ctx)?;
        sqlx::query("INSERT INTO wallet(name,uid,address,status,is_init,created_at) VALUES ('fixture','fixture','wallet',1,1,'2026-01-01T00:00:00Z')").execute(pool.write_ref()).await?;
        sqlx::query("INSERT INTO chain(name,chain_code,protocols,main_symbol,status,created_at) VALUES ('Scroll','scroll','[]','ETH',1,'2026-01-01T00:00:00Z')").execute(pool.write_ref()).await?;
        sqlx::query("INSERT INTO account(account_id,name,address,wallet_address,chain_code,status,is_init,created_at) VALUES (1,'fixture','owner','wallet','scroll',1,1,'2026-01-01T00:00:00Z')").execute(pool.write_ref()).await?;
        let api_asset_count = sqlx::query_scalar("SELECT COUNT(*) FROM api_assets")
            .fetch_one(mqtt::api_wallet_pool(ctx)?.read_ref())
            .await?;
        Ok(Self {
            ctx,
            pool,
            recorder,
            _dir: dir,
            api_asset_count,
            events: std::sync::Mutex::new(receiver),
        })
    }
    fn page(&self, symbol: &str, token: &str) {
        self.recorder.succeed_next_backend_call_with_data(serde_json::json!({
            "totalPage":2,"pageSize":500,"totalCount":2,"list":[{
                "id":symbol,"code":symbol,"name":symbol,"chainCode":"scroll","contractAddress":token,
                "unit":6,"enable":true,"defaultToken":true,"popularToken":true,"protocol":"ERC20","price":1.0,
                "createTime":"2026-10-08 00:00:00","updateTime":"2026-10-08 00:00:00"
            }]
        }));
    }
    pub fn given_two_successful_pages(&self) {
        self.page("USDT", "0xtoken-a");
        self.page("USDC", "0xtoken-b");
        self.recorder.succeed_next_backend_call_with_data(serde_json::json!({"list":[]}));
    }
    pub fn given_failed_last_page(&self) {
        self.page("USDT", "0xtoken-a");
        self.recorder.fail_next_api_backend_call(500, "controlled last-page failure");
    }
    pub fn given_failed_catalog_completion(&self) {
        self.page("USDT", "0xtoken-a");
        self.page("USDC", "0xtoken-b");
        self.recorder.fail_next_api_backend_call(500, "controlled completion failure");
    }
    pub fn given_last_page_paused(&self) -> Arc<MockResponseGate> {
        self.recorder.hold_next_response().release();
        self.recorder.hold_next_response()
    }
    pub fn when_sync_starts(
        &self,
    ) -> tokio::task::JoinHandle<Result<(), wallet_api::error::service::ServiceError>> {
        let ctx = self.ctx;
        tokio::spawn(async move { chain_sync::pull_ordinary_coins(ctx).await })
    }
    pub async fn when_sync_runs(&self) -> anyhow::Result<()> {
        chain_sync::pull_ordinary_coins(self.ctx).await?;
        Ok(())
    }
    pub async fn then_contract_assets_are(&self, expected: i64) -> anyhow::Result<()> {
        let actual: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE chain_code='scroll' AND token_address IN ('0xtoken-a','0xtoken-b') AND is_multisig=0 AND status=1 AND balance='0'").fetch_one(self.pool.read_ref()).await?;
        assert_eq!(actual, expected);
        Ok(())
    }
    pub async fn then_contract_coins_are(&self, expected: i64) -> anyhow::Result<()> {
        let actual: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM coin WHERE chain_code='scroll' AND token_address IN ('0xtoken-a','0xtoken-b') AND is_default=1").fetch_one(self.pool.read_ref()).await?;
        assert_eq!(actual, expected);
        Ok(())
    }
    pub async fn then_api_assets_are_unchanged(&self) -> anyhow::Result<()> {
        let actual: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_assets")
            .fetch_one(mqtt::api_wallet_pool(self.ctx)?.read_ref())
            .await?;
        assert_eq!(actual, self.api_asset_count);
        Ok(())
    }
    pub fn then_refresh_signals_are(&self, expected: usize) {
        let mut receiver = self.events.lock().unwrap();
        let mut count = 0;
        while let Ok(event) = receiver.try_recv() {
            assert_eq!(event.event, "SYNC_ASSETS");
            count += 1;
        }
        assert_eq!(count, expected);
    }
    pub fn then_only_catalog_endpoints_were_called(&self) {
        let calls = self.recorder.snapshot();
        assert_eq!(calls.len(), 6);
        assert_eq!(
            calls.iter().filter(|c| c.path.ends_with("token/queryAllExcludeCustomByPage")).count(),
            4
        );
        assert!(calls.iter().all(|c| c.path.contains("token/")));
    }
}
