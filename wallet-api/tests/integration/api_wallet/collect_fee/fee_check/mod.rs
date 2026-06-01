mod support;

use serial_test::serial;

use support::CollectFeeCheckScenario;

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_fails_on_uninitialized_recipient() {
    let scenario = CollectFeeCheckScenario::new().await;

    let _guard = scenario.given_sol_recipient_adapter(true, 27_309_206);
    let collect = scenario
        .given_sol_collect_order(
            "T_collect_sol_rent_fail",
            "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
        )
        .await;

    let err = scenario.when_sol_fee_check_fails(&collect).await;

    scenario.then_error_mentions_uninitialized_recipient(err);
    scenario.then_collect_status_is_init(&collect).await;
}

#[serial]
#[tokio::test]
async fn collect_sol_native_fee_check_allows_initialized_recipient() {
    let scenario = CollectFeeCheckScenario::new().await;

    let _guard = scenario.given_sol_recipient_adapter(false, 27_309_206);
    let collect = scenario
        .given_sol_collect_order(
            "T_collect_sol_rent_ok",
            "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
        )
        .await;

    let pass = scenario.when_sol_fee_check_runs(&collect).await;

    scenario.then_fee_check_passed(pass);
    scenario.then_collect_status_is_init(&collect).await;
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_with_partial_oracle_fallback() {
    let scenario = CollectFeeCheckScenario::new().await;

    let _guard = scenario.given_eth_fee_adapter(100_000_000_000_000_000u128, 0.00000368);
    let collect = scenario
        .given_eth_collect_order(
            "T_collect_eth_partial_oracle",
            "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
            "0xFCa230313618af2a33fa00455D8A5d1466C91332",
            "0.000015",
        )
        .await;

    let pass = scenario.when_eth_fee_check_runs(&collect).await;

    scenario.then_fee_check_passed(pass);
    scenario.then_collect_status_is_init(&collect).await;
}

#[serial]
#[tokio::test]
async fn collect_eth_native_fee_check_fails_on_insufficient_balance() {
    let scenario = CollectFeeCheckScenario::new().await;

    let _guard = scenario.given_eth_fee_adapter(10_000_000_000_000u128, 0.00000368);
    let collect = scenario
        .given_eth_collect_order(
            "T_collect_eth_insufficient",
            "0x477000C778C66FaAA36596Fb846Ce34C89bc652D",
            "0xFCa230313618af2a33fa00455D8A5d1466C91332",
            "0.000015",
        )
        .await;

    let pass = scenario.when_eth_fee_check_runs(&collect).await;

    scenario.then_fee_check_failed(pass, "insufficient ETH balance should fail the fee check");
    scenario.then_collect_status_is_init(&collect).await;
}

#[serial]
#[tokio::test]
async fn collect_build_fee_estimation_shortage_reopens_fee_cycle() {
    let scenario = CollectFeeCheckScenario::new().await;

    let _guard = scenario.given_sol_fee_shortage_adapter(false, 0);
    let collect = scenario
        .given_sol_collect_order(
            "T_collect_fee_shortage",
            "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
        )
        .await;

    let pass = scenario.when_sol_fee_check_runs(&collect).await;
    scenario.then_fee_check_failed(
        pass,
        "fee estimate shortage must reopen the service-fee cycle instead of erroring",
    );

    let affected = scenario.when_build_attempt_is_invalidated(&collect).await;

    scenario.then_one_row_was_affected(affected);
    scenario.then_fee_cycle_is_reopened(&collect).await;
}
