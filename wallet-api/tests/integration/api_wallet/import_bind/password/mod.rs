mod support;

use serial_test::serial;

use support::PasswordRotationScenario;

#[tokio::test]
#[serial(import_bind)]
async fn change_password_refreshes_api_wallet_unlock_session() {
    let scenario = PasswordRotationScenario::new().await;

    scenario.given_backend_accepts_withdrawal_uid();

    let uid = scenario.when_withdrawal_wallet_is_imported().await;

    scenario.then_wallet_is_withdrawal(&uid).await;
    scenario.when_password_is_rotated().await;
    scenario.when_rotation_tick_passes().await;
    scenario.when_api_wallet_chain_data_syncs().await;
    scenario.when_password_is_restored().await;
}
