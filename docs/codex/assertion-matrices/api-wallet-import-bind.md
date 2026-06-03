# API Wallet Import And Bind Assertion Matrix

Scope: `wallet-api` API wallet import, bind, and API-wallet password flows.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Import Subaccount Wallet

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/import_subaccount.rs
  import_subaccount_wallet_ok_unbound
  import_subaccount_wallet_sets_progress_stage_before_completion
  ```

- Backend: fake API-wallet backend reports `ApiRaw` and unbound UID info.
- DB facts: subaccount wallet is persisted with `sn`, empty app/merchant
  fields, and final `import_stage = 3`.
- Invariant: a successful subaccount import must finish local import state and
  call UID check, bind-info query, init, and old-keys init paths.

### Import Subaccount Rejection

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/import_subaccount.rs
  import_subaccount_wallet_query_failure_does_not_persist_half_state
  import_subaccount_wallet_uid_status_mismatch_rejected_without_persist
  ```

- Backend: fake backend either fails bind-info query or reports `ApiWaw`.
- DB facts: expected UID does not exist after rejection.
- Invariant: rejected subaccount import must not persist half-state or continue
  into key initialization after preflight failure.

### Import Withdrawal Wallet

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/import_withdrawal.rs
  import_withdrawal_wallet_ok_requires_binding_address
  import_withdrawal_wallet_recovers_incomplete_subaccount_then_completes
  import_withdrawal_wallet_reimport_keeps_completion_and_account_count_stable
  import_withdrawal_wallet_with_concurrent_asset_reads_succeeds
  ```

- Backend: fake backend reports `ApiWaw`, bind-info, and app-id usage facts.
- DB facts: withdrawal and recharge wallets bind to each other, share app and
  merchant ids, and finish `import_stage = 3`.
- Invariant: withdrawal import must bind to an existing recharge wallet, recover
  incomplete local state, tolerate reimport, and remain readable while import is
  in progress.

### Import Withdrawal Rejection

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/import_withdrawal.rs
  import_withdrawal_wallet_uid_usage_false_rejected_without_persist
  ```

- Backend: fake backend reports `ApiWaw`, bind-info, and app-id usage `false`.
- DB facts: expected withdrawal UID does not exist after rejection.
- Invariant: withdrawal import must not persist a wallet or initialize keys when
  app-id usage validation fails.

### Bind Relation

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/bind_relation.rs
  scan_bind_ok_calls_backend_and_persists_bind_sn_and_relation
  import_bind_ok_calls_appid_import_and_persists_bind_sn_and_relation
  ```

- Backend: fake backend records one scan bind or app-id import call.
- DB facts: both wallets persist `sn`, app id, merchant id, and reciprocal
  binding addresses.
- Invariant: successful bind APIs must persist the same relation fields on both
  wallet records.

### Bind Relation Rejection

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/bind_relation.rs
  import_bind_backend_fail_does_not_persist_relation
  scan_bind_backend_fail_does_not_persist_bind
  import_bind_missing_wallet_returns_not_found_and_no_backend_call
  scan_bind_remote_first_then_persist
  ```

- Backend: fake backend can fail scan bind or app-id import.
- DB facts: existing bind fields remain unchanged on backend failure or missing
  wallet.
- Invariant: remote bind/import must happen before local relation persistence,
  and missing wallet must not trigger backend import.

### API Wallet Password Refresh

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/import_bind/password.rs
  change_password_refreshes_api_wallet_unlock_session
  ```

- Backend: fake backend reports `ApiWaw`.
- DB facts: withdrawal wallet imports successfully before password rotation.
- Invariant: password rotation must refresh the API-wallet unlock session so a
  later sync can still run.

## Template Contract

`import_bind/mod.rs` is a directory-level organizer:

1. Keep import, bind relation, and password tests in flow-specific files.
2. Keep only shared import phrases and password constants in
   `import_bind/support.rs`.
3. Keep direct harness and fake-backend setup inside the owning subflow file
   until a subflow needs deeper scenario extraction.
4. Preserve existing test names so targeted filters remain stable.

`import_bind/password/mod.rs` uses a password-refresh integration contract:

1. Keep password-refresh test cases read-first in `password/mod.rs`.
2. Use `PasswordRotationScenario` with shared `ScenarioRoles`.
3. Keep password-flow business methods in flow-local
   `PasswordRotationGiven`, `PasswordRotationWhen`, and
   `PasswordRotationThen` traits.
4. Use `scenario.given()` for fake backend UID status.
5. Use `scenario.when()` for wallet import, password rotation, timer tick,
   chain-data sync, and password restoration.
6. Use `scenario.then()` for persisted wallet type assertions.
7. Keep harness environment reset, manager calls, backend queue setup, waits,
   and DB loads below the test body in `password/support/scenario.rs`.

`import_bind/import_subaccount/mod.rs` uses the role-trait Given-When-Then
shape:

1. Keep subaccount import cases read-first in `import_subaccount/mod.rs`.
2. Keep fake backend environment in `SubaccountImportScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep backend UID status and bind-info setup in the flow-local
   `SubaccountImportGiven` trait.
5. Keep import success and failure entrypoints in the flow-local
   `SubaccountImportWhen` trait.
6. Keep wallet DB facts, persisted import stage, error facts, and backend-call
   assertions in the flow-local `SubaccountImportThen` trait.
7. Keep manager calls, DB loads, and load/assert internals below the
   Given-When-Then test body.

`import_bind/import_withdrawal/mod.rs` uses the role-trait Given-When-Then
shape:

1. Keep withdrawal import cases read-first in `import_withdrawal/mod.rs`.
2. Keep fake backend environment in `WithdrawalImportScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep backend UID status, bind-info, app-id usage setup, import delay, and
   recharge wallet seeding in the flow-local `WithdrawalImportGiven` trait.
5. Keep import success/failure entrypoints, concurrent asset reads, and delay
   cleanup guard in the flow-local `WithdrawalImportGiven` trait.
6. Keep wallet binding facts, completion facts, reimport stability, concurrent
   read result, rejection facts, and backend-call assertions in the flow-local
   `WithdrawalImportThen` trait.
7. Keep manager calls, DB loads, fake backend queues, and concurrency helpers
   below the Given-When-Then test body.

`import_bind/bind_relation/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep bind relation cases read-first in `bind_relation/mod.rs`.
2. Keep fake backend environment in `BindRelationScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep backend failure setup, wallet pair seeding, single-wallet seeding, and
   bind snapshots in the flow-local `BindRelationGiven` trait.
5. Keep scan bind, import bind, and missing-wallet entrypoints in the
   flow-local `BindRelationWhen` trait.
6. Keep persisted bind fields, unchanged snapshots, error facts, and
   exact backend-call assertions in the flow-local `BindRelationThen` trait.
7. Keep manager calls, DB loads, snapshot helpers, and backend recorder checks
   below the Given-When-Then test body.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Extract scenario helpers for the withdrawal import flow if more import
  recovery tests are added.
- Add explicit bind relation concurrency coverage if the local persistence
  order changes.
