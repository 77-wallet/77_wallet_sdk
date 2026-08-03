use std::{cell::RefCell, time::Duration};

use tokio::task::JoinHandle;
use wallet_api::error::service::ServiceError;

use crate::harness::{
    AssertRole, GivenRole, LoadRole, SMOKE_WALLET_PASSWORD, SeedRole, TestEnv, ThenRole, WhenRole,
    ensure_env,
};

use super::{
    adapter::{AdapterGuard, RecordingEthAdapter, install_adapter},
    db::{ensure_bnb_transfer_fixture, load_bnb_nonce_floor},
    request::{BNB_TO_ADDR, make_transfer_req},
};

pub(crate) type TransferTask = JoinHandle<Result<String, ServiceError>>;

pub(crate) struct TransferNonceScenario {
    env: &'static TestEnv,
    from_addr: RefCell<String>,
    adapter: RecordingEthAdapter,
    guard: RefCell<Option<AdapterGuard>>,
}

impl TransferNonceScenario {
    pub(crate) async fn new() -> Self {
        Self {
            env: ensure_env().await,
            from_addr: RefCell::new(String::new()),
            adapter: RecordingEthAdapter::new(),
            guard: RefCell::new(None),
        }
    }

    fn seed(&self) -> SeedRole<'_, Self> {
        SeedRole::new(self)
    }

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait TransferNonceGiven {
    async fn bnb_transfer_fixture(&self) -> anyhow::Result<()>;

    fn first_transfer_blocks(&self);

    fn transfer_fails(&self);

    fn fake_chain_adapter(&self);

    async fn wallet_password_cached(&self);
}

#[async_trait::async_trait(?Send)]
impl TransferNonceGiven for GivenRole<'_, TransferNonceScenario> {
    async fn bnb_transfer_fixture(&self) -> anyhow::Result<()> {
        self.scenario().seed().bnb_transfer_fixture().await
    }

    fn first_transfer_blocks(&self) {
        self.scenario().seed().first_transfer_blocks();
    }

    fn transfer_fails(&self) {
        self.scenario().seed().transfer_fails();
    }

    fn fake_chain_adapter(&self) {
        self.scenario().seed().fake_chain_adapter();
    }

    async fn wallet_password_cached(&self) {
        self.scenario().seed().wallet_password_cached().await;
    }
}

#[async_trait::async_trait(?Send)]
trait TransferNonceSeed {
    async fn bnb_transfer_fixture(&self) -> anyhow::Result<()>;

    fn first_transfer_blocks(&self);

    fn transfer_fails(&self);

    fn fake_chain_adapter(&self);

    async fn wallet_password_cached(&self);
}

#[async_trait::async_trait(?Send)]
impl TransferNonceSeed for SeedRole<'_, TransferNonceScenario> {
    async fn bnb_transfer_fixture(&self) -> anyhow::Result<()> {
        let from_addr = ensure_bnb_transfer_fixture(self.scenario().env).await?;
        self.scenario().from_addr.replace(from_addr);
        Ok(())
    }

    fn first_transfer_blocks(&self) {
        self.scenario().adapter.block_first_transfer();
    }

    fn transfer_fails(&self) {
        self.scenario().adapter.fail_on_transfer();
    }

    fn fake_chain_adapter(&self) {
        self.scenario().guard.replace(Some(install_adapter(&self.scenario().adapter)));
    }

    async fn wallet_password_cached(&self) {
        let _ = self.scenario().env.manager.set_passwd_cache(SMOKE_WALLET_PASSWORD).await;
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait TransferNonceWhen {
    fn transfer_starts(&self) -> TransferTask;

    async fn transfer_fails(&self) -> ServiceError;

    fn first_transfer_is_released(&self);
}

#[async_trait::async_trait(?Send)]
impl TransferNonceWhen for WhenRole<'_, TransferNonceScenario> {
    fn transfer_starts(&self) -> TransferTask {
        let wallet_manager = self.scenario().env.manager.clone();
        let from_addr = self.scenario().from_addr.borrow().clone();
        let req = make_transfer_req(&from_addr, BNB_TO_ADDR);

        tokio::spawn(async move {
            let resp =
                wallet_manager.api_transfer_with_preloaded_private_key(req, "00".into()).await?;
            Ok(resp.tx_hash)
        })
    }

    async fn transfer_fails(&self) -> ServiceError {
        let from_addr = self.scenario().from_addr.borrow().clone();
        let req = make_transfer_req(&from_addr, BNB_TO_ADDR);
        self.scenario()
            .env
            .manager
            .api_transfer_with_preloaded_private_key(req, "00".into())
            .await
            .expect_err("transfer should fail")
    }

    fn first_transfer_is_released(&self) {
        self.scenario().adapter.release_first();
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait TransferNonceThen {
    async fn first_transfer_has_entered(&self);

    async fn only_first_nonce_is_recorded_while_second_waits(&self);

    async fn serial_transfer_results_are(
        &self,
        first: TransferTask,
        second: TransferTask,
    ) -> anyhow::Result<()>;

    async fn failure_keeps_reserved_nonce(&self, err: ServiceError) -> anyhow::Result<()>;
}

#[async_trait::async_trait(?Send)]
impl TransferNonceThen for ThenRole<'_, TransferNonceScenario> {
    async fn first_transfer_has_entered(&self) {
        self.scenario().adapter.wait_for_first_entry().await;
    }

    async fn only_first_nonce_is_recorded_while_second_waits(&self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let recorded_nonces = self.scenario().adapter.recorded_nonces();
        self.scenario().assert().only_first_nonce_is_recorded_while_second_waits(&recorded_nonces);
    }

    async fn serial_transfer_results_are(
        &self,
        first: TransferTask,
        second: TransferTask,
    ) -> anyhow::Result<()> {
        let first_result = first.await.expect("first join")?;
        let second_result = second.await.expect("second join")?;
        let recorded_nonces = self.scenario().adapter.recorded_nonces();

        self.scenario().assert().serial_transfer_results_are(
            &first_result,
            &second_result,
            &recorded_nonces,
        );

        Ok(())
    }

    async fn failure_keeps_reserved_nonce(&self, err: ServiceError) -> anyhow::Result<()> {
        let from_addr = self.scenario().from_addr.borrow().clone();
        let floor = self.scenario().load().bnb_nonce_floor(&from_addr).await?;
        let recorded_nonces = self.scenario().adapter.recorded_nonces();

        self.scenario().assert().failure_keeps_reserved_nonce(err, floor, &recorded_nonces);

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
trait TransferNonceLoad {
    async fn bnb_nonce_floor(&self, from_addr: &str) -> anyhow::Result<i64>;
}

#[async_trait::async_trait(?Send)]
impl TransferNonceLoad for LoadRole<'_, TransferNonceScenario> {
    async fn bnb_nonce_floor(&self, from_addr: &str) -> anyhow::Result<i64> {
        load_bnb_nonce_floor(self.scenario().env, from_addr).await
    }
}

trait TransferNonceAssert {
    fn only_first_nonce_is_recorded_while_second_waits(&self, recorded_nonces: &[u64]);

    fn serial_transfer_results_are(
        &self,
        first_result: &str,
        second_result: &str,
        recorded_nonces: &[u64],
    );

    fn failure_keeps_reserved_nonce(
        &self,
        err: ServiceError,
        nonce_floor: i64,
        recorded_nonces: &[u64],
    );
}

impl TransferNonceAssert for AssertRole<'_, TransferNonceScenario> {
    fn only_first_nonce_is_recorded_while_second_waits(&self, recorded_nonces: &[u64]) {
        assert_eq!(recorded_nonces, [1], "second transfer should stay blocked");
    }

    fn serial_transfer_results_are(
        &self,
        first_result: &str,
        second_result: &str,
        recorded_nonces: &[u64],
    ) {
        assert_eq!(first_result, format!("0x{:064x}", 1u64));
        assert_eq!(second_result, format!("0x{:064x}", 2u64));
        assert_eq!(recorded_nonces, [1, 2]);
    }

    fn failure_keeps_reserved_nonce(
        &self,
        err: ServiceError,
        nonce_floor: i64,
        recorded_nonces: &[u64],
    ) {
        assert!(err.to_string().contains("simulated transfer failure"), "unexpected error: {err}");
        assert_eq!(nonce_floor, 1, "nonce floor should stay advanced after failure");
        assert_eq!(recorded_nonces, [1]);
    }
}
