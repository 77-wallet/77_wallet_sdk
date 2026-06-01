use wallet_api::error::service::ServiceError;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use crate::harness::{self, ApiWalletBackendCall, ensure_env, reset_fake};

use super::{
    db::{find_wallet, load_wallet, persisted_import_stage},
    fixtures::SubaccountImportFixture,
};

use super::super::super::support::{SUBACCOUNT_PHRASE, WALLET_PASSWORD};

pub(crate) struct SubaccountImportScenario {
    env: &'static harness::TestEnv,
}

impl SubaccountImportScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);
        Self { env }
    }

    pub(crate) fn given_backend_accepts_unbound_subaccount(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
        self.env.fake_backend.enqueue_query_uid_bind_info("", "", false, &self.env.sn);
    }

    pub(crate) fn given_backend_bind_info_query_fails(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
        self.env.fake_backend.set_query_uid_bind_info_error(Some("bind-info timeout"));
    }

    pub(crate) fn given_backend_reports_withdrawal_uid_status(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    }

    pub(crate) async fn when_subaccount_wallet_is_imported(
        &self,
        import: &SubaccountImportFixture,
    ) -> String {
        self.import_subaccount_wallet(import).await.expect("import subaccount wallet")
    }

    pub(crate) async fn when_subaccount_wallet_import_fails(
        &self,
        import: &SubaccountImportFixture,
    ) -> ServiceError {
        self.import_subaccount_wallet(import).await.expect_err("subaccount import should fail")
    }

    pub(crate) async fn then_subaccount_wallet_is_unbound_and_initialized(&self, uid: &str) {
        let wallet = load_wallet(self.env, uid).await;
        assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::SubAccount as u8);
        assert_eq!(wallet.sn.as_deref(), Some(self.env.sn.as_str()));
        assert_eq!(wallet.merchant_id.as_deref(), Some(""));
        assert_eq!(wallet.app_id.as_deref(), Some(""));
    }

    pub(crate) async fn then_bind_info_failure_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    ) {
        let err_msg = format!("{err:?}");
        assert!(err_msg.contains("bind-info timeout"));
        assert!(
            find_wallet(self.env, &import.expected_uid).await.is_none(),
            "wallet record should not be persisted when preflight query fails"
        );
    }

    pub(crate) async fn then_import_returns_expected_uid_and_completes_stage(
        &self,
        uid: &str,
        import: &SubaccountImportFixture,
    ) {
        assert_eq!(uid, import.expected_uid);

        let wallet = load_wallet(self.env, uid).await;
        assert_eq!(wallet.import_stage, 3);
        assert_eq!(persisted_import_stage(self.env, uid).await, Some(3));
    }

    pub(crate) async fn then_uid_status_mismatch_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    ) {
        let (code, _msg): (i64, String) = err.into();
        assert_eq!(code, 20002, "unexpected error code for uid status mismatch");
        assert!(
            find_wallet(self.env, &import.expected_uid).await.is_none(),
            "wallet record should not persist when uid type mismatches"
        );
    }

    pub(crate) fn then_standard_import_backend_calls_were_sent(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
            assert!(
                calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. }))
            );
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    pub(crate) fn then_bind_info_failure_backend_calls_were_sent(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
            assert!(
                calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. }))
            );
            assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    pub(crate) fn then_only_uid_check_was_called(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert_eq!(calls.len(), 1, "only uid check should run");
            assert!(matches!(calls[0], ApiWalletBackendCall::KeysUidCheck { .. }));
        });
    }

    async fn import_subaccount_wallet(
        &self,
        import: &SubaccountImportFixture,
    ) -> Result<String, ServiceError> {
        self.env
            .manager
            .import_api_wallet(
                1,
                SUBACCOUNT_PHRASE,
                &import.salt,
                &import.wallet_name,
                WALLET_PASSWORD,
                None,
                ApiWalletType::SubAccount,
                None,
            )
            .await
    }
}
