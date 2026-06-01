use std::time::Duration;

use tokio::task::JoinHandle;
use wallet_api::error::service::ServiceError;

use crate::harness::{self, SMOKE_WALLET_PASSWORD, ensure_env};

use super::{
    adapter::{AdapterGuard, RecordingEthAdapter, install_adapter},
    db::{ensure_bnb_transfer_fixture, load_bnb_nonce_floor},
    request::{BNB_TO_ADDR, make_transfer_req},
};

pub(crate) struct TransferNonceScenario {
    env: &'static harness::TestEnv,
    from_addr: String,
    adapter: RecordingEthAdapter,
    _guard: Option<AdapterGuard>,
}

impl TransferNonceScenario {
    pub(crate) async fn new() -> Self {
        Self {
            env: ensure_env().await,
            from_addr: String::new(),
            adapter: RecordingEthAdapter::new(),
            _guard: None,
        }
    }

    pub(crate) async fn given_bnb_transfer_fixture(&mut self) -> anyhow::Result<()> {
        self.from_addr = ensure_bnb_transfer_fixture(self.env).await?;
        Ok(())
    }

    pub(crate) fn given_first_transfer_blocks(&self) {
        self.adapter.block_first_transfer();
    }

    pub(crate) fn given_transfer_fails(&self) {
        self.adapter.fail_on_transfer();
    }

    pub(crate) fn given_fake_chain_adapter(&mut self) {
        self._guard = Some(install_adapter(&self.adapter));
    }

    pub(crate) async fn given_wallet_password_cached(&self) {
        let _ = self.env.manager.set_passwd_cache(SMOKE_WALLET_PASSWORD).await;
    }

    pub(crate) fn when_transfer_starts(&self) -> JoinHandle<Result<String, ServiceError>> {
        let wallet_manager = self.env.manager.clone();
        let req = make_transfer_req(&self.from_addr, BNB_TO_ADDR);

        tokio::spawn(async move {
            let resp =
                wallet_manager.api_transfer_with_preloaded_private_key(req, "00".into()).await?;
            Ok(resp.tx_hash)
        })
    }

    pub(crate) async fn when_transfer_fails(&self) -> ServiceError {
        let req = make_transfer_req(&self.from_addr, BNB_TO_ADDR);
        self.env
            .manager
            .api_transfer_with_preloaded_private_key(req, "00".into())
            .await
            .expect_err("transfer should fail")
    }

    pub(crate) async fn then_first_transfer_has_entered(&self) {
        self.adapter.wait_for_first_entry().await;
    }

    pub(crate) async fn then_only_first_nonce_is_recorded_while_second_waits(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(self.adapter.recorded_nonces(), vec![1], "second transfer should stay blocked");
    }

    pub(crate) fn when_first_transfer_is_released(&self) {
        self.adapter.release_first();
    }

    pub(crate) async fn then_serial_transfer_results_are(
        &self,
        first: JoinHandle<Result<String, ServiceError>>,
        second: JoinHandle<Result<String, ServiceError>>,
    ) -> anyhow::Result<()> {
        let first_result = first.await.expect("first join")?;
        let second_result = second.await.expect("second join")?;

        assert_eq!(first_result, format!("0x{:064x}", 1u64));
        assert_eq!(second_result, format!("0x{:064x}", 2u64));
        assert_eq!(self.adapter.recorded_nonces(), vec![1, 2]);

        Ok(())
    }

    pub(crate) async fn then_failure_keeps_reserved_nonce(
        &self,
        err: ServiceError,
    ) -> anyhow::Result<()> {
        assert!(err.to_string().contains("simulated transfer failure"), "unexpected error: {err}");

        let floor = load_bnb_nonce_floor(self.env, &self.from_addr).await?;
        assert_eq!(floor, 1, "nonce floor should stay advanced after failure");
        assert_eq!(self.adapter.recorded_nonces(), vec![1]);

        Ok(())
    }
}
