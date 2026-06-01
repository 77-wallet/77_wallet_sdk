# Testing Templates

This document is the copyable test template playbook. Use it after deciding the
test layer from `docs/codex/testing.md`.

Goals:

- Make new tests look the same across modules.
- Keep integration tests readable as `given -> when -> then` business
  scenarios.
- Keep unit and component tests independent from integration harnesses.
- Keep smoke/live tests opt-in and clearly separated.

## Layer Decision

Choose the smallest layer that can prove the behavior.

- Unit: one pure rule, parser, mapper, status decision, or error mapping.
- Component: one module plus local SQLite/repository/domain collaboration.
- Integration: one business flow with fake backend, temp DB, and side effects.
- Smoke/live: real backend, real chain RPC, fixed addresses, or local secrets.

Do not use a higher layer only because a helper already exists there.

## File Shape

Simple unit/component files may keep helpers and tests together. When a file is
small enough to read at once, use this order:

1. Imports.
2. Constants.
3. Local fixtures and seed helpers.
4. Local assertion helpers.
5. Tests.

For integration flows where support code would hide the test story, use a
read-first directory shape:

```text
<crate>/tests/integration/<module>/<flow>/
  mod.rs       # read-first test cases
  support.rs   # flow-local fixture, scenario, SQL, backend, payload, assertions
```

`mod.rs` should contain only:

1. `mod support;`
2. Imports from `support`.
3. Test cases.

`support.rs` should contain the details a reviewer can skip on first read:

1. Fixture structs.
2. Scenario structs and the public `given_*` / `when_*` / `then_*` API.
3. Scenario-private helpers such as `seed_*`, `persist_*`, `load_*`, `count_*`.
4. Payload builders and assertion helpers.

Keep `support.rs` local to that flow. Move code to `tests/harness` only after
at least two flows need the same environment or fake capability.

Do not put unrelated flows in the same file. Split by the business question the
file answers, not by the old location it came from.

## Naming

Use behavior names:

```text
<flow>_<condition>_<expected_result>
```

Examples:

- `withdraw_tx_ack_sends_once_and_persists_fact`
- `withdraw_tx_ack_backend_failure_keeps_fact_unset_and_retryable`
- `withdraw_confirm_failure_writes_chain_failed_fact`
- `collect_receipt_duplicate_upload_is_idempotent`

Avoid generic names:

- `test_transfer`
- `test_success`
- `test_1`
- `smoke_test`

## Style Choice

Use the smallest style that stays readable:

- Unit: Arrange-Act-Assert. Keep the tested rule visible.
- Component: Arrange-Act-Assert. Small `given_*` helpers are allowed for noisy
  DB setup, but the test still focuses on one module.
- Integration: Given-When-Then. The test body should read like a business
  scenario and hide SQLite, JSON, channel, and backend recorder plumbing.
- Smoke/live: Arrange-Act-Assert. Keep manual live checks direct and explicit.

Given-When-Then is a naming convention for integration helpers:

- `given_*`: prepare business facts or fake behavior.
- `when_*`: execute one manager entrypoint, worker step, scanner step, or retry.
- `then_*`: assert DB facts, backend calls, notifications, scanner labels,
  idempotency, retryability, or ordering.

Lower-level helpers such as `seed_*`, `persist_*`, `load_*`, `count_*`, and
`assert_*` may exist below the scenario layer. Integration test bodies should
call the business-level `given_*`, `when_*`, and `then_*` methods instead.

## Integration Template

Use this for standard integration tests under:

```text
<crate>/tests/integration/<module>/<flow>.rs
<crate>/tests/integration/<module>/<flow>/mod.rs
```

Template:

```rust
use serial_test::serial;

use crate::harness::{ensure_worker_env, next_unique_id};

struct FlowFixture {
    trade_no: String,
}

impl FlowFixture {
    fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self { trade_no: format!("T_{prefix}_{id}") }
    }
}

struct FlowScenario {
    env: &'static WorkerTestEnv,
}

impl FlowScenario {
    async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();
        Self { env }
    }

    async fn given_target_order(&self, fixture: &FlowFixture) {
        seed_target_row(&self.env.db_dir, &fixture.trade_no).await;
    }

    async fn given_backend_next_call_fails(&self) {
        self.env
            .recorder
            .fail_next_api_backend_call(503, "temporary failure");
    }

    async fn when_target_step_runs(
        &self,
        fixture: &FlowFixture,
    ) -> Result<(), ServiceError> {
        run_target_step(&fixture.trade_no).await
    }

    async fn then_backend_was_called_once(&self, fixture: &FlowFixture) {
        assert_backend_call(&self.env.recorder, &fixture.trade_no);
    }

    async fn then_flow_is_retryable(&self, fixture: &FlowFixture) {
        let saved = load_target_row(&self.env.db_dir, &fixture.trade_no).await;
        assert!(saved.retry_fact.is_none());
    }
}

fn then_step_finished_without_crashing(result: Result<(), ServiceError>) {
    result.expect("target step should finish without crashing");
}

#[serial]
#[tokio::test]
async fn flow_condition_expected_result() {
    let scenario = FlowScenario::new().await;
    let fixture = FlowFixture::new("flow_case");

    scenario.given_target_order(&fixture).await;
    scenario.given_backend_next_call_fails().await;

    let result = scenario.when_target_step_runs(&fixture).await;

    then_step_finished_without_crashing(result);
    scenario.then_backend_was_called_once(&fixture).await;
    scenario.then_flow_is_retryable(&fixture).await;
}
```

Rules:

- Use `<flow>.rs` for short flows. Use `<flow>/mod.rs` plus `support.rs` when
  fixtures, SQL setup, backend recorder handling, payload conversion, or
  assertions make the file hard to scan.
- The test body must read as Given-When-Then.
- Prefer business names over technical names in test bodies.
- Use one primary act. If a second act is needed, it must prove retry,
  idempotency, or ordering.
- Assert both DB facts and backend calls when the flow has both.
- Use unique `trade_no`, `uid`, and addresses.
- Reset fake state at the start of each test.
- Do not use real backend, real chain RPC, or fixed `test_data`.
- Do not expose `SqliteContext`, JSON serialization, channel plumbing, or
  backend recorder decryption in the test body.
- In directory-shaped flows, `mod.rs` is the review entrypoint and `support.rs`
  is the flow-local detail boundary.

## API Wallet Integration Scenario Template

Use this shape for API wallet worker, notification, ACK, receipt, and retry
flows. The first standard example is:

```text
wallet-api/tests/integration/api_wallet/withdraw_notification.rs
```

Local roles:

- `*Scenario`: owns one test environment view, DB pools, fake backend recorder,
  and flow-specific actions.
- `*Fixture`: creates unique immutable input data, such as `uid`, `trade_no`,
  addresses, and fixed test amounts.
- `given_*`: prepares business facts, DB rows, fake behavior, or notification
  collectors.
- `when_*`: executes one business entrypoint, worker step, scanner step, or
  retry.
- `then_*`: checks result, DB facts, backend calls, notifications, scanner
  labels, retryability, idempotency, or ordering.
- `seed_*`, `persist_*`, `load_*`, and `assert_*`: lower-level private helpers
  below the scenario layer.

Copyable structure:

Use this in `<flow>/support.rs` when the flow is large enough to split.

```rust
struct FlowFixture {
    uid: String,
    trade_no: String,
}

impl FlowFixture {
    fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self {
            uid: format!("uid_{prefix}_{id}"),
            trade_no: format!("T_{prefix}_{id}"),
        }
    }
}

struct FlowScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl FlowScenario {
    async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(&env.db_dir).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    async fn given_flow_row(&self, fixture: &FlowFixture) {
        // Insert only the facts required by this flow.
    }

    fn given_backend_next_call_fails(&self, status: u16, body: &str) {
        self.env.recorder.fail_next_api_backend_call(status, body);
    }

    async fn when_target_step_runs(
        &self,
        trade_no: &str,
    ) -> Result<(), ServiceError> {
        // Call one manager entrypoint or one testkit worker step.
    }

    async fn then_flow_is_retryable(&self, fixture: &FlowFixture) {
        // Assert DB facts, backend calls, or scanner state.
    }

    async fn then_backend_attempted_once(&self, trade_no: &str) {
        // Assert captured backend call count and payload.
    }
}

fn then_step_finished_without_crashing(result: Result<(), ServiceError>) {
    result.expect("target step should finish without crashing");
}

#[serial]
#[tokio::test]
async fn flow_condition_expected_result() {
    let scenario = FlowScenario::new().await;
    let fixture = FlowFixture::new("flow_case");

    scenario.given_flow_row(&fixture).await;
    scenario.given_backend_next_call_fails(503, "temporary failure");

    let result = scenario
        .when_target_step_runs(&fixture.trade_no)
        .await;

    then_step_finished_without_crashing(result);
    scenario
        .then_backend_attempted_once(&fixture.trade_no)
        .await;
    scenario.then_flow_is_retryable(&fixture).await;
}
```

Rules:

- Keep `Scenario` local until a helper is proven useful to another flow.
- Prefer a short `mod.rs` that imports `FlowFixture` and `FlowScenario` from
  `support.rs`, then lists the test cases.
- `Scenario` may hold `WorkerTestEnv`, DB pools, and notification collectors;
  it must not hide the business assertion being proven.
- `Fixture` should be cheap, unique, and immutable after construction.
- Use `tests/harness` only for cross-flow environment and fake capabilities.
- Use `src/testkit` only for crate-private worker or scanner entrypoints.
- Keep the test body as the readable spec; helpers should remove plumbing, not
  the core business expectation.
- Test bodies should call `given_*`, `when_*`, and `then_*`; lower-level
  `seed_*`, `persist_*`, `load_*`, and `assert_*` helpers stay below the
  scenario layer.

## Component Template

Use this for source-side module tests that need SQLite or repositories but not
backend, chain RPC, manager, or global context.

Location:

```text
<crate>/src/.../<module>.rs
```

Template:

```rust
#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    struct TestDb {
        _dir: TempDir,
        pool: TargetDbPool,
    }

    impl TestDb {
        async fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let pool = open_temp_pool(dir.path()).await;
            Self { _dir: dir, pool }
        }

        async fn seed_target(&self, trade_no: &str) {
            seed_row(&self.pool, trade_no).await;
        }
    }

    #[tokio::test]
    async fn flow_condition_writes_expected_fact() {
        let db = TestDb::new().await;
        let trade_no = "T_COMPONENT_CASE";
        db.seed_target(trade_no).await;

        let outcome = TargetDomain::target_step(&db.pool, trade_no)
            .await
            .expect("target step");

        assert!(outcome.should_continue);
        let saved = load_target(&db.pool, trade_no).await;
        assert!(saved.expected_fact.is_some());
        assert!(saved.forbidden_fact.is_none());
    }
}
```

Rules:

- Use temp SQLite only.
- Do not call `WalletManager::new`.
- Do not read or write fixed `test_data`.
- Do not depend on `tests/harness`.
- Assert real persisted DB fields, not only returned values.

## Unit Template

Use this for pure rules and single-step decisions.

Template:

```rust
#[test]
fn rule_condition_returns_expected_decision() {
    let input = Fixture::new()
        .with_required_fact()
        .without_blocking_fact()
        .build();

    let decision = decide_next_step(&input);

    assert_eq!(decision.stage, ExpectedStage::Ready);
    assert_eq!(decision.next_fact, Some("expected_fact"));
}
```

Rules:

- No SQLite.
- No network.
- No manager.
- No global context.
- Prefer small fixtures over large object graphs.

## Smoke Template

Use this only for real backend, real chain RPC, fixed addresses, local config,
or manual operator checks.

Location:

```text
<crate>/tests/smoke/<module>/<flow>.rs
```

Template:

```rust
#[tokio::test]
#[ignore = "requires live backend, chain RPC, and local smoke config"]
async fn live_flow_reaches_remote_system() {
    let config = load_local_smoke_config()
        .expect("create tests/smoke/<module>/<flow>.local.toml");

    let manager = create_live_manager(&config).await;
    let result = manager.live_entrypoint().await;

    assert!(result.is_ok());
}
```

Rules:

- Every smoke/live test must have `#[ignore = "..."]`.
- Do not print private keys, mnemonics, tokens, or production config.
- Keep local config files ignored by git.
- Smoke tests do not replace unit, component, or integration coverage.

## Helper Ownership

Prefer local helpers first.

Use `src/testkit/` when integration tests need a stable test-only entrypoint
into crate-private worker, scanner, or domain steps. Do not put environment
setup there.

Keep helpers inside the same test file when they serve one flow:

- `seed_withdraw`
- `assert_withdraw_fact`
- `count_withdraw_tx_ack_requests`

Move helpers into `tests/harness/` only when at least two modules or flows use
the same capability:

- fake backend recorder
- temp DB environment
- decrypt captured backend body
- notification collector

Harness helpers must not contain business decisions. They prepare data, fake
dependencies, run a step, or assert observations.

`testkit` helpers must not create the integration test environment. They expose
internal steps so integration tests can call them intentionally.

Recommended helper prefixes:

- `given_*`: prepare business facts or fake behavior for integration tests.
- `when_*`: execute one target action in integration tests.
- `then_*`: assert one business outcome in integration tests.
- `seed_*`: insert data.
- `prepare_*`: configure data plus fake behavior.
- `run_*`: execute one target step.
- `count_*`: count observed side effects.
- `assert_*`: assert DB facts or side effects.
- `load_*`: load current DB state.

Avoid vague helper names:

- `do_test`
- `mock_data`
- `common`
- `helper`

## Assertion Matrix Sync

Whenever a flow test changes, update the matching file under:

```text
docs/codex/assertion-matrices/
```

Each matrix entry should state:

- flow and test entrypoint
- layer
- expected backend calls
- expected DB facts
- failure invariant
- remaining coverage gaps

If there is no matrix for the flow yet, add a small one for only that flow.

## Migration Order

After old tests are classified into `integration` or `smoke`, standardize one
flow at a time:

1. Pick one file, such as `withdraw_notification.rs`.
2. Rename tests to behavior names.
3. Shape integration tests into Given-When-Then business scripts.
4. Pull repeated seed/assert code into scenario-local helpers.
5. Move only proven cross-flow helpers into `tests/harness/`.
6. Update the assertion matrix.
7. Run the smallest target command.

Do not standardize multiple modules in the same batch.
