# Testing Templates

This document is the copyable test template playbook. Use it after deciding the
test layer from `docs/codex/testing.md`.

Goals:

- Make new tests look the same across modules.
- Keep integration tests readable as `arrange -> act -> assert`.
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

Every test file should follow this order:

1. Imports.
2. Constants.
3. Local fixtures and seed helpers.
4. Local assertion helpers.
5. Tests.

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

## Integration Template

Use this for standard integration tests under:

```text
<crate>/tests/integration/<module>/<flow>.rs
```

Template:

```rust
use serial_test::serial;

use crate::harness::{
    ensure_worker_env,
    next_unique_id,
    open_api_wallet_pool,
};

#[serial]
#[tokio::test]
async fn flow_condition_expected_result() {
    // Arrange: environment
    let env = ensure_worker_env().await;
    env.recorder.reset();

    let trade_no = format!("T_flow_{}", next_unique_id());
    let tx_pool = open_transaction_pool(&env.db_dir).await;
    let core_pool = open_api_wallet_pool(&env.db_dir).await;

    // Arrange: data and fake behavior
    seed_target_row(&tx_pool, &trade_no).await;
    env.recorder.fail_next_api_backend_call(503, "temporary failure");

    // Act: execute one business entrypoint or one worker step
    run_target_step(tx_pool.clone(), core_pool, &trade_no)
        .await
        .expect("target step should finish without crashing");

    // Assert: returned result, durable facts, and external side effects
    assert_target_fact(&tx_pool, &trade_no).await;
    assert_backend_call(&env.recorder, &trade_no);

    // Assert: retry, idempotency, or ordering when the flow requires it
    assert_retryable_or_idempotent(&tx_pool, &trade_no).await;
}
```

Rules:

- The test body must be readable in four blocks:
  `Arrange: environment`, `Arrange: data`, `Act`, `Assert`.
- Use one primary act. If a second act is needed, it must prove retry,
  idempotency, or ordering.
- Assert both DB facts and backend calls when the flow has both.
- Use unique `trade_no`, `uid`, and addresses.
- Reset fake state at the start of each test.
- Do not use real backend, real chain RPC, or fixed `test_data`.

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
3. Shape each test into arrange, act, assert blocks.
4. Pull repeated seed/assert code into local helpers.
5. Move only proven cross-flow helpers into `tests/harness/`.
6. Update the assertion matrix.
7. Run the smallest target command.

Do not standardize multiple modules in the same batch.
