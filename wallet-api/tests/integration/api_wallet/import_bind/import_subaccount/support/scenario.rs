use wallet_api::error::service::ServiceError;
use wallet_database::entities::api_wallet::{ApiWalletEntity, ApiWalletType};
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use crate::harness::{
    self, ApiWalletBackendCall, AssertRole, GivenRole, LoadRole, ThenRole, WhenRole, ensure_env,
    reset_fake,
};

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

    fn load(&self) -> LoadRole<'_, Self> {
        LoadRole::new(self)
    }

    fn assert(&self) -> AssertRole<'_, Self> {
        AssertRole::new(self)
    }
}

pub(crate) trait SubaccountImportGiven {
    fn backend_accepts_unbound_subaccount(&self);

    fn backend_bind_info_query_fails(&self);

    fn backend_reports_withdrawal_uid_status(&self);
}

impl SubaccountImportGiven for GivenRole<'_, SubaccountImportScenario> {
    fn backend_accepts_unbound_subaccount(&self) {
        self.scenario().env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
        self.scenario().env.fake_backend.enqueue_query_uid_bind_info(
            "",
            "",
            false,
            &self.scenario().env.sn,
        );
    }

    fn backend_bind_info_query_fails(&self) {
        self.scenario().env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiRaw);
        self.scenario().env.fake_backend.set_query_uid_bind_info_error(Some("bind-info timeout"));
    }

    fn backend_reports_withdrawal_uid_status(&self) {
        self.scenario().env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait SubaccountImportWhen {
    async fn subaccount_wallet_is_imported(&self, import: &SubaccountImportFixture) -> String;

    async fn subaccount_wallet_import_fails(
        &self,
        import: &SubaccountImportFixture,
    ) -> ServiceError;
}

#[async_trait::async_trait(?Send)]
impl SubaccountImportWhen for WhenRole<'_, SubaccountImportScenario> {
    async fn subaccount_wallet_is_imported(&self, import: &SubaccountImportFixture) -> String {
        self.import_subaccount_wallet(import).await.expect("import subaccount wallet")
    }

    async fn subaccount_wallet_import_fails(
        &self,
        import: &SubaccountImportFixture,
    ) -> ServiceError {
        self.import_subaccount_wallet(import).await.expect_err("subaccount import should fail")
    }
}

impl WhenRole<'_, SubaccountImportScenario> {
    async fn import_subaccount_wallet(
        &self,
        import: &SubaccountImportFixture,
    ) -> Result<String, ServiceError> {
        self.scenario()
            .env
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

#[async_trait::async_trait(?Send)]
pub(crate) trait SubaccountImportThen {
    async fn subaccount_wallet_is_unbound_and_initialized(&self, uid: &str);

    async fn bind_info_failure_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    );

    async fn import_returns_expected_uid_and_completes_stage(
        &self,
        uid: &str,
        import: &SubaccountImportFixture,
    );

    async fn uid_status_mismatch_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    );

    fn standard_import_backend_calls_were_sent(&self);

    fn bind_info_failure_backend_calls_were_sent(&self);

    fn only_uid_check_was_called(&self);
}

#[async_trait::async_trait(?Send)]
impl SubaccountImportThen for ThenRole<'_, SubaccountImportScenario> {
    async fn subaccount_wallet_is_unbound_and_initialized(&self, uid: &str) {
        let wallet = self.scenario().load().wallet(uid).await;
        self.scenario()
            .assert()
            .subaccount_wallet_is_unbound_and_initialized(&wallet, &self.scenario().env.sn);
    }

    async fn bind_info_failure_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    ) {
        let wallet = self.scenario().load().maybe_wallet(&import.expected_uid).await;
        self.scenario().assert().bind_info_failure_did_not_persist(err, wallet);
    }

    async fn import_returns_expected_uid_and_completes_stage(
        &self,
        uid: &str,
        import: &SubaccountImportFixture,
    ) {
        let wallet = self.scenario().load().wallet(uid).await;
        let persisted_stage = self.scenario().load().persisted_import_stage(uid).await;
        self.scenario().assert().import_returns_expected_uid_and_completes_stage(
            uid,
            import,
            &wallet,
            persisted_stage,
        );
    }

    async fn uid_status_mismatch_did_not_persist(
        &self,
        err: ServiceError,
        import: &SubaccountImportFixture,
    ) {
        let wallet = self.scenario().load().maybe_wallet(&import.expected_uid).await;
        self.scenario().assert().uid_status_mismatch_did_not_persist(err, wallet);
    }

    fn standard_import_backend_calls_were_sent(&self) {
        self.scenario().assert().standard_import_backend_calls_were_sent();
    }

    fn bind_info_failure_backend_calls_were_sent(&self) {
        self.scenario().assert().bind_info_failure_backend_calls_were_sent();
    }

    fn only_uid_check_was_called(&self) {
        self.scenario().assert().only_uid_check_was_called();
    }
}

#[async_trait::async_trait(?Send)]
trait SubaccountImportLoad {
    async fn wallet(&self, uid: &str) -> ApiWalletEntity;

    async fn maybe_wallet(&self, uid: &str) -> Option<ApiWalletEntity>;

    async fn persisted_import_stage(&self, uid: &str) -> Option<u8>;
}

#[async_trait::async_trait(?Send)]
impl SubaccountImportLoad for LoadRole<'_, SubaccountImportScenario> {
    async fn wallet(&self, uid: &str) -> ApiWalletEntity {
        load_wallet(self.scenario().env, uid).await
    }

    async fn maybe_wallet(&self, uid: &str) -> Option<ApiWalletEntity> {
        find_wallet(self.scenario().env, uid).await
    }

    async fn persisted_import_stage(&self, uid: &str) -> Option<u8> {
        persisted_import_stage(self.scenario().env, uid).await
    }
}

trait SubaccountImportAssert {
    fn subaccount_wallet_is_unbound_and_initialized(&self, wallet: &ApiWalletEntity, sn: &str);

    fn bind_info_failure_did_not_persist(&self, err: ServiceError, wallet: Option<ApiWalletEntity>);

    fn import_returns_expected_uid_and_completes_stage(
        &self,
        uid: &str,
        import: &SubaccountImportFixture,
        wallet: &ApiWalletEntity,
        persisted_stage: Option<u8>,
    );

    fn uid_status_mismatch_did_not_persist(
        &self,
        err: ServiceError,
        wallet: Option<ApiWalletEntity>,
    );

    fn standard_import_backend_calls_were_sent(&self);

    fn bind_info_failure_backend_calls_were_sent(&self);

    fn only_uid_check_was_called(&self);
}

impl SubaccountImportAssert for AssertRole<'_, SubaccountImportScenario> {
    fn subaccount_wallet_is_unbound_and_initialized(&self, wallet: &ApiWalletEntity, sn: &str) {
        assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::SubAccount as u8);
        assert_eq!(wallet.sn.as_deref(), Some(sn));
        assert_eq!(wallet.merchant_id.as_deref(), Some(""));
        assert_eq!(wallet.app_id.as_deref(), Some(""));
    }

    fn bind_info_failure_did_not_persist(
        &self,
        err: ServiceError,
        wallet: Option<ApiWalletEntity>,
    ) {
        let err_msg = format!("{err:?}");
        assert!(err_msg.contains("bind-info timeout"));
        assert!(
            wallet.is_none(),
            "wallet record should not be persisted when preflight query fails"
        );
    }

    fn import_returns_expected_uid_and_completes_stage(
        &self,
        uid: &str,
        import: &SubaccountImportFixture,
        wallet: &ApiWalletEntity,
        persisted_stage: Option<u8>,
    ) {
        assert_eq!(uid, import.expected_uid);
        assert_eq!(wallet.import_stage, 3);
        assert_eq!(persisted_stage, Some(3));
    }

    fn uid_status_mismatch_did_not_persist(
        &self,
        err: ServiceError,
        wallet: Option<ApiWalletEntity>,
    ) {
        let (code, _msg): (i64, String) = err.into();
        assert_eq!(code, 20002, "unexpected error code for uid status mismatch");
        assert!(wallet.is_none(), "wallet record should not persist when uid type mismatches");
    }

    fn standard_import_backend_calls_were_sent(&self) {
        self.scenario().env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
            assert!(
                calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. }))
            );
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::InitApiWallet(_))));
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    fn bind_info_failure_backend_calls_were_sent(&self) {
        self.scenario().env.fake_backend.with_calls(|calls| {
            assert!(calls.iter().any(|c| matches!(c, ApiWalletBackendCall::KeysUidCheck { .. })));
            assert!(
                calls.iter().any(|c| matches!(c, ApiWalletBackendCall::QueryUidBindInfo { .. }))
            );
            assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::OldKeysInit(_))));
        });
    }

    fn only_uid_check_was_called(&self) {
        self.scenario().env.fake_backend.with_calls(|calls| {
            assert_eq!(calls.len(), 1, "only uid check should run");
            assert!(matches!(calls[0], ApiWalletBackendCall::KeysUidCheck { .. }));
        });
    }
}
