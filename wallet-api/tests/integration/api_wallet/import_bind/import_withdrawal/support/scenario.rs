use std::time::Duration;

use tokio::task::JoinHandle;
use wallet_api::error::service::ServiceError;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use crate::harness::{self, ApiWalletBackendCall, ensure_env, reset_fake};

use super::{
    db::{find_wallet, load_wallet, persisted_import_stage, seed_recharge_wallet},
    fixtures::{RechargeWalletFixture, WithdrawalImportFixture},
};

use super::super::super::support::{WALLET_PASSWORD, WITHDRAWAL_PHRASE};

const APP_WITHDRAW: &str = "app-withdraw";
const MERCHANT_WITHDRAW: &str = "merchant-withdraw";

pub(crate) struct WithdrawalImportScenario {
    env: &'static harness::TestEnv,
}

impl WithdrawalImportScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);
        Self { env }
    }

    pub(crate) fn given_backend_accepts_withdrawal_import(&self, bound: bool) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
        self.enqueue_withdraw_bind_info(APP_WITHDRAW, MERCHANT_WITHDRAW, bound, 2);
        self.enqueue_uid_usage_used(4);
    }

    pub(crate) fn given_backend_accepts_withdrawal_reimport(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
        self.enqueue_uid_usage_used(14);
        self.enqueue_withdraw_bind_info(APP_WITHDRAW, MERCHANT_WITHDRAW, true, 6);
    }

    pub(crate) fn given_backend_rejects_uid_usage(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
        self.enqueue_withdraw_bind_info("app-usage-check", "merchant-usage-check", true, 1);
        self.env.fake_backend.enqueue_appid_uid_usage_used(false);
    }

    pub(crate) fn given_backend_appid_import_delay(&self) {
        self.env.fake_backend.set_appid_import_delay(Some(Duration::from_millis(80)));
    }

    pub(crate) async fn given_recharge_wallet(
        &self,
        uid_prefix: &str,
        import_stage: u8,
    ) -> RechargeWalletFixture {
        seed_recharge_wallet(self.env, uid_prefix, import_stage).await
    }

    pub(crate) async fn when_withdrawal_wallet_is_imported(
        &self,
        import: &WithdrawalImportFixture,
    ) -> String {
        self.import_withdrawal_wallet(import).await.expect("import withdrawal wallet")
    }

    pub(crate) async fn when_withdrawal_wallet_import_fails(
        &self,
        import: &WithdrawalImportFixture,
    ) -> ServiceError {
        self.import_withdrawal_wallet(import).await.expect_err("withdrawal import should fail")
    }

    pub(crate) fn when_asset_reads_start(&self, address: &str) -> JoinHandle<usize> {
        let manager = &self.env.manager;
        let query_address = address.to_string();
        tokio::spawn(async move {
            let mut ok_count = 0;
            for _ in 0..12 {
                let res = manager.get_api_wallet_assets(Some(&query_address), None, None).await;
                if res.is_ok() {
                    ok_count += 1;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            ok_count
        })
    }

    pub(crate) fn when_backend_appid_import_delay_is_cleared(&self) {
        self.env.fake_backend.set_appid_import_delay(None);
    }

    pub(crate) async fn then_wallets_are_bound_with_backend_fields(
        &self,
        withdrawal_uid: &str,
        recharge: &RechargeWalletFixture,
    ) {
        let withdrawal_wallet = load_wallet(self.env, withdrawal_uid).await;
        let recharge_wallet = load_wallet(self.env, &recharge.uid).await;
        assert_eq!(withdrawal_wallet.api_wallet_type as u8, ApiWalletType::Withdrawal as u8);
        assert_eq!(
            withdrawal_wallet.binding_address.as_deref(),
            Some(recharge_wallet.address.as_str())
        );
        assert_eq!(
            recharge_wallet.binding_address.as_deref(),
            Some(withdrawal_wallet.address.as_str())
        );
        assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some(MERCHANT_WITHDRAW));
        assert_eq!(withdrawal_wallet.app_id.as_deref(), Some(APP_WITHDRAW));
        assert_eq!(recharge_wallet.merchant_id.as_deref(), Some(MERCHANT_WITHDRAW));
        assert_eq!(recharge_wallet.app_id.as_deref(), Some(APP_WITHDRAW));
    }

    pub(crate) async fn then_recharge_and_withdrawal_completed_and_bound(
        &self,
        withdrawal_uid: &str,
        recharge: &RechargeWalletFixture,
    ) {
        let recharge_wallet = load_wallet(self.env, &recharge.uid).await;
        let withdrawal_wallet = load_wallet(self.env, withdrawal_uid).await;
        assert_eq!(recharge_wallet.import_stage, 3);
        assert_eq!(withdrawal_wallet.import_stage, 3);
        assert_eq!(
            withdrawal_wallet.binding_address.as_deref(),
            Some(recharge_wallet.address.as_str())
        );
        assert_eq!(recharge_wallet.app_id.as_deref(), Some(APP_WITHDRAW));
        assert_eq!(recharge_wallet.merchant_id.as_deref(), Some(MERCHANT_WITHDRAW));
        assert_eq!(withdrawal_wallet.app_id.as_deref(), Some(APP_WITHDRAW));
        assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some(MERCHANT_WITHDRAW));
    }

    pub(crate) async fn then_reimport_keeps_completion_and_uid_stable(
        &self,
        first_uid: &str,
        second_uid: &str,
    ) {
        assert_eq!(first_uid, second_uid);

        let wallet = load_wallet(self.env, first_uid).await;
        assert_eq!(wallet.import_stage, 3);
        assert_eq!(persisted_import_stage(self.env, first_uid).await, Some(3));
    }

    pub(crate) async fn then_asset_reads_saw_successes(&self, reads: JoinHandle<usize>) {
        let ok_reads = reads.await.expect("asset read task");
        assert!(ok_reads > 0, "expected concurrent read task to observe successful reads");
    }

    pub(crate) async fn then_uid_usage_rejection_did_not_persist(
        &self,
        err: ServiceError,
        import: &WithdrawalImportFixture,
    ) {
        let (code, _msg): (i64, String) = err.into();
        assert_eq!(code, 20004, "unexpected error code for withdrawal uid usage check");
        assert!(
            find_wallet(self.env, &import.expected_uid).await.is_none(),
            "withdrawal record should not persist on appid usage check failure"
        );
    }

    pub(crate) fn then_standard_import_backend_calls_were_sent(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdUidUsage(_))));
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    pub(crate) fn then_uid_usage_rejection_backend_calls_were_sent(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
            assert!(
                calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. }))
            );
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdUidUsage(_))));
            assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
            assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    async fn import_withdrawal_wallet(
        &self,
        import: &WithdrawalImportFixture,
    ) -> Result<String, ServiceError> {
        self.env
            .manager
            .import_api_wallet(
                1,
                WITHDRAWAL_PHRASE,
                &import.salt,
                &import.wallet_name,
                WALLET_PASSWORD,
                None,
                ApiWalletType::Withdrawal,
                Some(&import.binding_address),
            )
            .await
    }

    fn enqueue_withdraw_bind_info(
        &self,
        app_id: &str,
        merchant_id: &str,
        bound: bool,
        count: usize,
    ) {
        for _ in 0..count {
            self.env.fake_backend.enqueue_query_uid_bind_info(
                app_id,
                merchant_id,
                bound,
                &self.env.sn,
            );
        }
    }

    fn enqueue_uid_usage_used(&self, count: usize) {
        for _ in 0..count {
            self.env.fake_backend.enqueue_appid_uid_usage_used(true);
        }
    }
}
