mod support;

use serial_test::serial;

use support::CollectBuildFeeScenario;

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_reopens_fee_cycle_on_first_insufficient_balance() {
    let scenario = CollectBuildFeeScenario::new().await;

    let _guard = scenario.given_low_balance_sol_adapter();
    let collect = scenario
        .given_sol_collect_order(
            "T_collect_fee_reopen_initial",
            "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
        )
        .await;

    let pass = scenario.when_fee_check_runs(&collect).await;
    scenario.then_fee_check_failed(
        pass,
        "low balance should still fail the build fee check on the first attempt",
    );

    let affected = scenario.when_build_attempt_is_invalidated(&collect).await;

    scenario.then_one_row_was_affected(affected);
    scenario.then_fee_cycle_is_reopened(&collect).await;
}

#[serial]
#[tokio::test]
async fn collect_build_fee_failure_preserves_completed_fee_cycle_facts() {
    let scenario = CollectBuildFeeScenario::new().await;

    let _guard = scenario.given_low_balance_sol_adapter();
    let collect = scenario
        .given_sol_collect_order(
            "T_collect_fee_reopen_rebuild",
            "3m2vk1NSfKJK444bCLFCtigFyeHP4cHgvLrtjCJr7nrW",
        )
        .await;
    scenario.given_completed_fee_cycle_facts(&collect).await;

    let collect = scenario.given_collect_reloaded(&collect).await;
    let pass = scenario.when_fee_check_runs(&collect).await;
    scenario.then_fee_check_failed(
        pass,
        "low balance should still fail the build fee check when fee facts already exist",
    );

    let affected = scenario.when_build_attempt_is_invalidated(&collect).await;

    scenario.then_one_row_was_affected(affected);
    scenario.then_completed_fee_cycle_facts_are_preserved(&collect).await;
}
