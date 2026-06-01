use wallet_api::error::service::ServiceError;

use crate::harness::{
    self, ApiWalletBackendCall, BindSnapshot, ExpectedBindReq, WalletPair, assert_bind_call_once,
    ensure_env, reset_fake,
};

use super::db::{
    PairBindSnapshot, load_wallet, seed_recharge_wallet, seed_wallet_pair,
    snapshot_pair_bind_fields, snapshot_wallet_bind_fields,
};

pub(crate) struct BindRelationScenario {
    env: &'static self::harness::TestEnv,
}

impl BindRelationScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);
        Self { env }
    }

    pub(crate) fn given_import_bind_backend_fails(&self, message: &str) {
        self.env.fake_backend.set_appid_import_error(Some(message));
    }

    pub(crate) fn given_scan_bind_backend_fails(&self, message: &str) {
        self.env.fake_backend.set_wallet_bind_appid_error(Some(message));
    }

    pub(crate) async fn given_wallet_pair(&self) -> WalletPair {
        seed_wallet_pair(self.env).await
    }

    pub(crate) async fn given_only_recharge_wallet(&self, uid_prefix: &str) -> String {
        seed_recharge_wallet(self.env, uid_prefix).await
    }

    pub(crate) async fn given_pair_bind_snapshot(&self, pair: &WalletPair) -> PairBindSnapshot {
        snapshot_pair_bind_fields(self.env, pair).await
    }

    pub(crate) async fn given_wallet_bind_snapshot(&self, uid: &str) -> BindSnapshot {
        snapshot_wallet_bind_fields(self.env, uid).await
    }

    pub(crate) async fn when_scan_bind_succeeds(
        &self,
        pair: &WalletPair,
        app_id: &str,
        merchant_id: &str,
    ) {
        self.env
            .manager
            .scan_bind(app_id, merchant_id, &pair.recharge_uid, &pair.withdrawal_uid)
            .await
            .expect("scan bind should succeed");
    }

    pub(crate) async fn when_import_bind_succeeds(
        &self,
        pair: &WalletPair,
        merchant_id: &str,
        app_id: &str,
    ) {
        self.env
            .manager
            .import_bind(
                self.env.sn.as_str(),
                merchant_id,
                app_id,
                &pair.recharge_uid,
                &pair.withdrawal_uid,
            )
            .await
            .expect("import bind should succeed");
    }

    pub(crate) async fn when_import_bind_fails(
        &self,
        pair: &WalletPair,
        merchant_id: &str,
        app_id: &str,
    ) -> ServiceError {
        self.env
            .manager
            .import_bind(
                self.env.sn.as_str(),
                merchant_id,
                app_id,
                &pair.recharge_uid,
                &pair.withdrawal_uid,
            )
            .await
            .expect_err("import_bind should fail")
    }

    pub(crate) async fn when_scan_bind_fails(
        &self,
        pair: &WalletPair,
        app_id: &str,
        merchant_id: &str,
    ) -> ServiceError {
        self.env
            .manager
            .scan_bind(app_id, merchant_id, &pair.recharge_uid, &pair.withdrawal_uid)
            .await
            .expect_err("scan_bind should fail")
    }

    pub(crate) async fn when_import_bind_missing_withdrawal_fails(
        &self,
        recharge_uid: &str,
    ) -> ServiceError {
        self.env
            .manager
            .import_bind(
                self.env.sn.as_str(),
                "missing-merchant",
                "missing-app",
                recharge_uid,
                "missing-withdrawal-uid",
            )
            .await
            .expect_err("import_bind should fail when wallet does not exist")
    }

    pub(crate) async fn then_pair_has_bind_fields(
        &self,
        pair: &WalletPair,
        app_id: &str,
        merchant_id: &str,
    ) {
        let recharge_wallet = load_wallet(self.env, &pair.recharge_uid).await;
        let withdrawal_wallet = load_wallet(self.env, &pair.withdrawal_uid).await;
        assert_eq!(recharge_wallet.app_id.as_deref(), Some(app_id));
        assert_eq!(withdrawal_wallet.app_id.as_deref(), Some(app_id));
        assert_eq!(recharge_wallet.merchant_id.as_deref(), Some(merchant_id));
        assert_eq!(withdrawal_wallet.merchant_id.as_deref(), Some(merchant_id));
        assert_eq!(recharge_wallet.sn.as_deref(), Some(self.env.sn.as_str()));
        assert_eq!(withdrawal_wallet.sn.as_deref(), Some(self.env.sn.as_str()));
        assert_eq!(
            recharge_wallet.binding_address.as_deref(),
            Some(pair.withdrawal_address.as_str())
        );
        assert_eq!(
            withdrawal_wallet.binding_address.as_deref(),
            Some(pair.recharge_address.as_str())
        );
    }

    pub(crate) async fn then_pair_bind_snapshot_is_unchanged(
        &self,
        pair: &WalletPair,
        before: PairBindSnapshot,
    ) {
        let after = snapshot_pair_bind_fields(self.env, pair).await;
        assert_eq!(after, before);
    }

    pub(crate) async fn then_missing_wallet_rejection_keeps_recharge_unchanged(
        &self,
        err: ServiceError,
        recharge_uid: &str,
        before: BindSnapshot,
    ) {
        let (code, _msg): (i64, String) = err.into();
        assert_eq!(code, 20001, "unexpected error code for missing uid");

        let after = snapshot_wallet_bind_fields(self.env, recharge_uid).await;
        assert_eq!(after, before, "existing wallet fields should remain unchanged");
    }

    pub(crate) fn then_error_contains(&self, err: ServiceError, expected: &str) {
        assert!(err.to_string().contains(expected));
    }

    pub(crate) fn then_scan_bind_backend_called_once(&self, pair: &WalletPair, app_id: &str) {
        assert_bind_call_once(
            &self.env.fake_backend,
            ExpectedBindReq {
                recharge_uid: pair.recharge_uid.clone(),
                withdrawal_uid: pair.withdrawal_uid.clone(),
                org_app_id: app_id.to_string(),
                sn: self.env.sn.clone(),
            },
        );
    }

    pub(crate) fn then_appid_import_backend_called_once(&self, pair: &WalletPair) {
        self.env.fake_backend.with_calls(|calls| {
            let appid_import_calls: Vec<_> = calls
                .iter()
                .filter_map(|call| match call {
                    ApiWalletBackendCall::AppIdImport(req) => Some(req),
                    _ => None,
                })
                .collect();
            assert_eq!(appid_import_calls.len(), 1);
            let req = appid_import_calls[0];
            assert_eq!(req.sn, self.env.sn);
            assert_eq!(req.recharge_uid.as_deref(), Some(pair.recharge_uid.as_str()));
            assert_eq!(req.withdrawal_uid.as_deref(), Some(pair.withdrawal_uid.as_str()));
        });
    }

    pub(crate) fn then_appid_import_backend_attempted_once(&self) {
        self.env.fake_backend.with_calls(|calls| {
            let appid_import_calls = calls
                .iter()
                .filter(|call| matches!(call, ApiWalletBackendCall::AppIdImport(_)))
                .count();
            assert_eq!(appid_import_calls, 1, "backend appid_import should be called exactly once");
        });
    }

    pub(crate) fn then_scan_bind_backend_attempted_once(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert_eq!(calls.len(), 1);
            assert!(matches!(calls[0], ApiWalletBackendCall::WalletBindAppId(_)));
        });
    }

    pub(crate) fn then_appid_import_backend_was_not_called(&self) {
        self.env.fake_backend.with_calls(|calls| {
            assert!(!calls.iter().any(|c| matches!(c, ApiWalletBackendCall::AppIdImport(_))));
        });
    }
}
