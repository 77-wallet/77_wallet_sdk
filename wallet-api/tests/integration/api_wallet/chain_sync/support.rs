use crate::harness::worker::{MockBackendRecorder, start_mock_backend_server};
use wallet_api::testkit::{chain_sync, mqtt};
use wallet_database::{
    entities::{
        api_account::CreateApiAccountVo,
        api_wallet::ApiWalletType,
        config::{
            AppVersion,
            config_key::{APP_VERSION, KEYS_RESET_EPOCH},
        },
    },
    repositories::{
        api_wallet::{account::ApiAccountRepo, chain::ApiChainRepo, coin::ApiCoinRepo},
        config::ConfigRepo,
    },
};
use wallet_transport_backend::{
    consts::endpoint::api_wallet::{ADDRESS_INIT, API_WALLET_CHAIN_LIST},
    request::{AddressInitReq, api_wallet::address::ApiAddressInitReq},
};

const CODES: [&str; 5] = ["polygon", "avalanche", "arbitrum", "base", "scroll"];
pub(super) struct ChainSyncScenario {
    ctx: &'static wallet_api::Context,
    recorder: MockBackendRecorder,
    _dir: tempfile::TempDir,
    config_text: String,
}

impl ChainSyncScenario {
    pub async fn new() -> anyhow::Result<Self> {
        // Reuse the shared HTTP fake without constructing background worker handles.
        let (url, recorder) = start_mock_backend_server()?;
        unsafe {
            std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
        }
        let config_text = format!(
            r#"
app_code: "test"
crypto: {{ aes_key: "1234567890abcdef", aes_iv: "abcdef1234567890" }}
backend_api: {{ dev_url: "{url}", test_url: "{url}", prod_url: "{url}" }}
aggregate_api: {{ dev_url: "{url}", test_url: "{url}", prod_url: "{url}" }}
oss: {{ access_key_id: "test", access_key_secret: "test", bucket_name: "test", endpoint: "test" }}
"#
        );
        let config = wallet_api::config::Config::new(&config_text)?;
        let dir = tempfile::tempdir()?;
        let ctx = Box::leak(Box::new(
            wallet_api::Context::new_without_handles_for_test(
                wallet_api::dirs::Dirs::new(dir.path().to_str().unwrap())?,
                config,
            )
            .await?,
        ));
        wallet_ecdh::GLOBAL_KEY.set_shared_secret(
            r#"-----BEGIN PUBLIC KEY-----
MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEWDZNP0ClbeWJey9hBr2rsjSayQEBywnv
ZXi0RberQCAp+06fOjvr+jZI5qwYGglmMkGJw49tbni6qgm4QNV6WQ==
-----END PUBLIC KEY-----"#,
        )?;
        wallet_ecdh::GLOBAL_KEY.set_sn("chain-sync-test");
        let api = mqtt::api_wallet_pool(ctx)?;
        // Every scenario owns a fresh migrated DB; cleanup only its five-chain fixtures.
        for table in ["api_chain", "api_coin", "api_account"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE chain_code IN ('polygon','avalanche','arbitrum','base','scroll')")).execute(api.write_ref()).await?;
        }
        let core = chain_sync::core_pool(ctx)?;
        ConfigRepo::upsert(
            APP_VERSION,
            &AppVersion { app_version: "2.2.0".into() }.to_json_str()?,
            Some(1),
            &core,
        )
        .await?;
        ConfigRepo::upsert(KEYS_RESET_EPOCH, "777", None, &core).await?;
        Self::configuration_without_background_dispatch(chain_sync::initialize(ctx).await)?;
        Ok(Self { ctx, recorder, _dir: dir, config_text })
    }
    fn configuration_without_background_dispatch(
        result: Result<(), wallet_api::error::service::ServiceError>,
    ) -> anyhow::Result<()> {
        // The real domain persists configuration/tasks before reporting absent handles.
        // This fixture deliberately starts no background dispatch; other errors propagate.
        match result {
            Ok(()) => Ok(()),
            Err(error) if error.to_string().contains("global handles not ready") => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn given_backend_disables_polygon_and_gates_base(&self) {
        self.recorder.succeed_next_api_backend_call_with_data(serde_json::json!([
            {"name":"Polygon", "code":"polygon", "enable":false, "masterTokenCode":"POL", "appVersionCode":"2.2.0"},
            {"name":"Base", "code":"base", "enable":true, "masterTokenCode":"ETH", "appVersionCode":"9.0.0"}
        ]));
    }
    pub async fn when_chain_sync_runs(&self) -> anyhow::Result<()> {
        chain_sync::sync_chains(self.ctx).await?;
        Ok(())
    }
    pub async fn then_disabled_chains_stay_disabled_after_reinitialize(
        &self,
    ) -> anyhow::Result<()> {
        Self::configuration_without_background_dispatch(chain_sync::initialize(self.ctx).await)?;
        let api = mqtt::api_wallet_pool(self.ctx)?;
        let enabled = ApiChainRepo::get_chain_list(&api).await?;
        assert!(!enabled.iter().any(|row| matches!(row.chain_code.as_str(), "polygon" | "base")));
        assert!(enabled.iter().any(|row| row.chain_code == "scroll"));
        Ok(())
    }
    pub fn then_one_chain_request_was_sent(&self) {
        let calls = self.recorder.snapshot();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].path.ends_with(API_WALLET_CHAIN_LIST));
    }
    pub async fn given_no_coins_and_invalid_coin_response(&self) -> anyhow::Result<()> {
        let api = mqtt::api_wallet_pool(self.ctx)?;
        sqlx::query("DELETE FROM api_coin WHERE chain_code IN ('polygon','avalanche','arbitrum','base','scroll')").execute(api.write_ref()).await?;
        self.recorder
            .succeed_next_api_backend_call_with_data(serde_json::json!({"content": "invalid"}));
        Ok(())
    }
    pub async fn when_coin_sync_runs(&self) -> anyhow::Result<()> {
        Ok(chain_sync::pull_coins(self.ctx).await?)
    }
    pub async fn then_coin_failure_has_no_default_write(
        &self,
        error: anyhow::Error,
    ) -> anyhow::Result<()> {
        assert!(!error.to_string().is_empty());
        let coins = ApiCoinRepo::coin_list(&mqtt::api_wallet_pool(self.ctx)?).await?;
        assert!(!coins.iter().any(|coin| CODES.contains(&coin.chain_code.as_str())));
        assert_eq!(self.recorder.snapshot().len(), 1);
        Ok(())
    }
    pub fn given_valid_empty_coin_response(&self) {
        let sort = serde_json::json!({"empty":true,"unsorted":true,"sorted":false});
        self.recorder.succeed_next_api_backend_call_with_data(serde_json::json!({
            "totalElements":0,"totalPages":1,"first":true,"last":true,"size":500,"number":0,
            "sort":sort,"pageable":{"offset":0,"sort":sort,"pageNumber":0,"pageSize":500,"unpaged":false,"paged":true},
            "numberOfElements":0,"empty":true,"content":[]
        }));
    }
    pub async fn then_five_native_coins_are_present(&self) -> anyhow::Result<()> {
        let coins: Vec<_> = ApiCoinRepo::coin_list(&mqtt::api_wallet_pool(self.ctx)?)
            .await?
            .into_iter()
            .filter(|coin| CODES.contains(&coin.chain_code.as_str()))
            .collect();
        assert_eq!(coins.len(), 5);
        assert!(coins.iter().all(|coin| CODES.contains(&coin.chain_code.as_str())
            && coin.token_address.is_native()
            && coin.decimals == 18));
        Ok(())
    }
    pub async fn given_five_unregistered_addresses_and_backend_failure(
        &self,
    ) -> anyhow::Result<()> {
        let api = mqtt::api_wallet_pool(self.ctx)?;
        for code in CODES {
            ApiAccountRepo::upsert_account_multi(
                &api,
                vec![CreateApiAccountVo::new(
                    0,
                    "test-address",
                    "test-pub",
                    "chain-sync-wallet",
                    "test-uid",
                    "m/44'/60'/0'/0/0",
                    0,
                    code,
                    "test",
                    ApiWalletType::SubAccount,
                )],
            )
            .await?;
        }
        self.recorder.fail_next_api_backend_call(503, "test unavailable");
        Ok(())
    }
    pub fn given_backend_registration_success(&self) {
        self.recorder.succeed_next_api_backend_call_with_data(serde_json::Value::Null);
    }
    pub async fn when_address_registration_runs(&self, epoch: u64) -> anyhow::Result<()> {
        let mut req = ApiAddressInitReq::new(epoch);
        for code in CODES {
            req.address_list.add_address(AddressInitReq::new(
                "test-uid",
                "test-address",
                0,
                code,
                "test-sn",
                vec!["".into()],
                "test",
            ));
        }
        Ok(chain_sync::register_addresses(self.ctx, req).await?)
    }
    pub async fn then_all_addresses_have_init_state(&self, state: i32) -> anyhow::Result<()> {
        let rows: Vec<_> = ApiAccountRepo::list(&mqtt::api_wallet_pool(self.ctx)?)
            .await?
            .into_iter()
            .filter(|row| {
                CODES.contains(&row.chain_code.as_str())
                    && row.wallet_address == "chain-sync-wallet"
            })
            .collect();
        assert_eq!(rows.len(), 5);
        assert!(rows.iter().all(|row| row.is_init == state));
        Ok(())
    }
    pub fn then_registration_call_count_is(&self, count: usize) {
        let calls = self.recorder.snapshot();
        assert_eq!(
            calls.len(),
            count,
            "recorded paths: {:?}",
            calls.iter().map(|call| call.path.as_str()).collect::<Vec<_>>()
        );
        assert!(calls.iter().all(|call| call.path.ends_with(ADDRESS_INIT)));
    }
}
