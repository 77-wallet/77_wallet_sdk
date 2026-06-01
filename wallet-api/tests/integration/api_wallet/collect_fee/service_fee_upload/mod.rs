mod support;

use serial_test::serial;

use support::CollectServiceFeeUploadScenario;

#[serial]
#[tokio::test]
async fn collect_service_fee_upload_bypasses_local_sol_fee_gate() {
    let scenario = CollectServiceFeeUploadScenario::new().await;

    let _guard = scenario.given_sol_fee_shortage_adapter(false);
    let upload =
        scenario.given_sol_service_fee_upload_waiting("T_collect_service_fee_upload").await;

    scenario
        .when_service_fee_is_uploaded(
            &upload,
            "service fee upload should bypass local balance gate",
        )
        .await;

    scenario.then_payload_routes_reverse_transfer(&upload);
    scenario.then_payload_amount_is(&upload, 0.00100588, "base Solana shortfall");
}

#[serial]
#[tokio::test]
async fn collect_service_fee_upload_includes_solana_recipient_ata_rent_when_missing() {
    let scenario = CollectServiceFeeUploadScenario::new().await;

    let _guard = scenario.given_sol_fee_shortage_adapter(true);
    let upload =
        scenario.given_sol_service_fee_upload_waiting("T_collect_service_fee_upload_ata").await;

    scenario
        .when_service_fee_is_uploaded(
            &upload,
            "service fee upload should include recipient ATA rent",
        )
        .await;

    scenario.then_payload_trade_no_matches(&upload);
    scenario.then_payload_amount_is(&upload, 0.00350588, "Solana recipient ATA rent");
}

#[serial]
#[tokio::test]
async fn collect_eth_service_fee_upload_uses_estimated_fee_without_multiplier() {
    let scenario = CollectServiceFeeUploadScenario::new().await;

    let _guard = scenario.given_eth_fee_adapter(100_000_000_000_000_000u128, 0.000015);
    let upload = scenario.given_eth_service_fee_upload_waiting().await;

    scenario
        .when_service_fee_is_uploaded(
            &upload,
            "eth service fee upload should use the estimated fee",
        )
        .await;

    scenario.then_payload_routes_reverse_transfer(&upload);
    scenario.then_payload_amount_is(&upload, 0.000015, "estimated ETH fee");
}
