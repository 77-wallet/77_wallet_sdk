use std::time::Duration;

use crate::harness::{ensure_env, load_wallet_by_uid, next_tag, reset_fake};
use serial_test::serial;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus;

use super::support::{WALLET_PASSWORD, WITHDRAWAL_PHRASE};

const ROTATED_PASSWORD: &str = "new_passwd";

#[tokio::test]
#[serial(import_bind)]
async fn change_password_refreshes_api_wallet_unlock_session() {
    // Scenario: password change refreshes the API-wallet unlock session, and a later
    // unlock-session-dependent sync still succeeds after the rotation tick.
    let env = ensure_env().await;
    reset_fake(env);
    env.fake_backend.enqueue_keys_uid_status(UidStatus::ApiWaw);

    let uid = env
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
        .expect("import withdrawal wallet");

    let wallet = load_wallet_by_uid(env, &uid).await;
    assert_eq!(wallet.api_wallet_type as u8, ApiWalletType::Withdrawal as u8);

    env.manager.set_all_password(WALLET_PASSWORD, ROTATED_PASSWORD).await.expect("change password");

    tokio::time::sleep(Duration::from_secs(2)).await;

    env.manager
        .sync_api_wallet_chain_data()
        .await
        .expect("sync api wallet chain data after password change");

    env.manager
        .set_all_password(ROTATED_PASSWORD, WALLET_PASSWORD)
        .await
        .expect("restore wallet password for later integration tests");
}
