use std::time::Duration;

use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use crate::harness::{self, ensure_env, load_wallet_by_uid, next_tag, reset_fake};

use super::super::super::support::{WALLET_PASSWORD, WITHDRAWAL_PHRASE};

const ROTATED_PASSWORD: &str = "new_passwd";

pub(crate) struct PasswordRotationScenario {
    env: &'static harness::TestEnv,
}

impl PasswordRotationScenario {
    pub(crate) async fn new() -> Self {
        let env = ensure_env().await;
        reset_fake(env);
        Self { env }
    }

    pub(crate) fn given_backend_accepts_withdrawal_uid(&self) {
        self.env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);
    }

    pub(crate) async fn when_withdrawal_wallet_is_imported(&self) -> String {
        self.env
            .manager
            .import_api_wallet(
                1,
                WITHDRAWAL_PHRASE,
                &next_tag("salt-password-change"),
                &next_tag("withdraw-wallet-password-change"),
                WALLET_PASSWORD,
                None,
                ApiWalletType::Withdrawal,
                None,
            )
            .await
            .expect("import withdrawal wallet")
    }

    pub(crate) async fn when_password_is_rotated(&self) {
        self.env
            .manager
            .set_all_password(WALLET_PASSWORD, ROTATED_PASSWORD)
            .await
            .expect("change password");
    }

    pub(crate) async fn when_rotation_tick_passes(&self) {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    pub(crate) async fn when_api_wallet_chain_data_syncs(&self) {
        self.env
            .manager
            .sync_api_wallet_chain_data()
            .await
            .expect("sync api wallet chain data after password change");
    }

    pub(crate) async fn when_password_is_restored(&self) {
        self.env
            .manager
            .set_all_password(ROTATED_PASSWORD, WALLET_PASSWORD)
            .await
            .expect("restore wallet password for later integration tests");
    }

    pub(crate) async fn then_wallet_is_withdrawal(&self, uid: &str) {
        let wallet = load_wallet_by_uid(self.env, uid).await;
        assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::Withdrawal as u8);
    }
}
