# API Wallet Collect Assertion Matrix

Scope: `wallet-api` API wallet collect flow.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Collect Order Notification Retry

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_notification/mod.rs
  collect_notification_retry_on_existing_trade_no
  ```

- Backend: none; the flow under test is frontend notification retry after a
  persisted collect order.
- DB facts: collect row is persisted with `status = Init` after the first
  notification failure.
- Notification: retry emits one frontend event with `event = COLLECT` and the
  expected `uid`, `fromAddr`, `toAddr`, and `value`.
- Invariant: failed frontend notification must not lose the persisted collect
  order, so retrying the same `trade_no` can notify again.

### Collect Resource Receipt Scanner

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  collect_scanner_emits_resource_receipt_upload_for_failed_delegation
  ```

- Backend: none.
- DB facts: failed resource delegation row is ready for receipt upload scan.
- Scanner: emits `UploadResourceTxExecReceipt`.
- Invariant: failed delegation rows must remain observable by the scanner.

### Collect Resource Result ACK Release

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  collect_resource_result_ack_releases_origin_collect_gate
  ```

- Backend: resource result ACK is sent for the delegation trade.
- DB facts: origin collect writes `resource_gate_released_at` and
  `resource_gate_result = ResourceDelegationSuccess`.
- Scanner: released collect becomes eligible for `BuildTx`.
- Invariant: only a successful collect-origin delegation releases the gate.

### Collect Resource Result ACK Failure

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  collect_resource_result_ack_does_not_release_gate_on_failure
  ```

- Backend: resource result ACK can be sent for the failed delegation.
- DB facts: origin collect keeps `resource_gate_released_at = NULL` and
  `resource_gate_result = NULL`.
- Invariant: failed delegation result ACK must not release the collect gate.

### Collect Gate Ignores Withdraw-Origin ACK

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  withdraw_origin_resource_result_ack_does_not_release_collect_gate
  ```

- Backend: resource result ACK can be sent.
- DB facts: collect gate remains unreleased.
- Invariant: withdraw-origin delegation must not release a collect gate.

### Collect Failed Resource Bypass

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  collect_failed_resource_bypass_reopens_collect_build_flow
  ```

- Backend: resource execution receipt upload is attempted for the delegation.
- DB facts: collect remains blocked on platform delegation dependency.
- Scanner: collect is still not eligible for `BuildTx`.
- Invariant: failed platform delegation must not reopen collect build before
  local fallback facts exist.

### Collect Resource Receipt Without Origin

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_resource_gate/mod.rs
  collect_resource_tx_exec_receipt_failure_without_origin_trade_no_does_not_release_gate
  ```

- Backend: resource execution receipt upload can run.
- DB facts: collect gate remains unreleased.
- Invariant: missing `origin_trade_no` must not release any collect gate.

### Collect Receipt Payload Uses Persisted Address

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_receipt/mod.rs
  collect_tx_exec_receipt_uses_persisted_to_addr
  ```

- Backend: none.
- DB facts: none; the test uses an in-memory `ApiCollectEntity`.
- Payload: `to`, `hash`, `tradeNo`, and `status` come from the persisted
  collect execution facts.
- Invariant: receipt payload must not rebuild `to` from request defaults after
  execution facts have been persisted.

### Collect Receipt Rebuild Uses Rebuilt Address

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_receipt/mod.rs
  collect_rebuild_then_receipt_upload_uses_rebuilt_to_addr
  ```

- Backend: none.
- DB facts: stale `raw_tx` and `tx_hash` are invalidated; rebuilt `to_addr` and
  `tx_hash` are loaded for the receipt payload.
- Payload: `to`, `hash`, `tradeNo`, and `status` reflect rebuilt execution
  facts.
- Invariant: receipt upload must use rebuilt execution facts, not stale build
  facts.

### Collect Receipt Worker Upload

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_receipt/mod.rs
  collect_side_effect_worker_marks_tx_exec_receipt_uploaded_after_rebuild
  ```

- Backend: mock backend receives execute-complete upload through the worker.
- DB facts: `tx_exec_receipt_uploaded_at` is set after successful backend
  upload.
- Payload: `tradeNo`, `to`, `hash`, and `status = SUCCESS` reflect rebuilt
  execution facts.
- Invariant: worker upload must write the durable upload fact only after the
  mock backend path succeeds.

### Collect Receipt Direct Backend Upload

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_receipt/mod.rs
  collect_backend_api_direct_upload_hits_mock_server
  ```

- Backend: mock backend captures `awallet/aw/trans/executeComplete`.
- DB facts: none; direct backend upload does not mutate the collect row.
- Payload: `tradeNo`, `to`, `hash`, and `status = SUCCESS` match the request
  entity.
- Invariant: direct backend helper must use the mock backend configured by the
  integration harness.

### Collect Receipt Scanner Dispatcher

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_receipt/mod.rs
  collect_scanner_dispatcher_uploads_rebuilt_tx_exec_receipt
  ```

- Backend: mock backend receives execute-complete upload through scanner
  dispatch.
- DB facts: scanner returns the selected `trade_no` and sets
  `tx_exec_receipt_uploaded_at`.
- Payload: `tradeNo`, `to`, `hash`, and `status = SUCCESS` reflect scanner-ready
  execution facts.
- Invariant: scanner-dispatcher must upload the rebuilt receipt once and
  persist the durable upload fact.

## Template Contract

`collect_notification/mod.rs` is the first V2 Given-When-Then gold sample:

1. Keep unique collect input data in `CollectOrderFixture`.
2. Keep environment, DB pool, notification setup, and collect actions in
   `CollectNotificationScenario`.
3. Keep notification receive and payload checks in `CollectNotificationInbox`.
4. Use `given_*` methods for wallet and notification-channel setup.
5. Use `when_*` methods for initial submit and retry.
6. Use `then_*` methods for surfaced error, DB retry fact, and notification
   payload.
7. Keep wallet seed, notification channel, DB pool, and payload serialization
   details below the scenario layer in `collect_notification/support.rs`.

`collect_receipt/mod.rs` uses a mixed-layer receipt contract:

1. Keep payload-only and local SQLite receipt tests as component-style
   Arrange-Act-Assert tests.
2. Keep unique integration receipt input data in `CollectReceiptFixture`.
3. Keep environment, DB pools, backend recorder, and receipt actions in
   `CollectReceiptScenario`.
4. Use `given_*` methods for mock-backend verification, rebuilt execution
   facts, and scanner-ready facts.
5. Use `when_*` methods for worker, direct backend, and scanner receipt upload
   entrypoints.
6. Use `then_*` methods for durable DB upload facts, receipt payload facts,
   backend execute-complete capture, and selected scanner trade.
7. Keep low-level details such as SQL updates, payload serialization, and
   backend body decryption below the scenario layer in
   `collect_receipt/support.rs`.

`collect_resource_gate/mod.rs` uses the same read-first shape for resource gate
flows:

1. Keep resource gate test cases in `collect_resource_gate/mod.rs`.
2. Keep unique trade data in `CollectResourceGateFixture`.
3. Keep local scanner-only DB setup in `LocalCollectResourceDb`.
4. Keep environment, DB pools, resource delegation setup, worker entrypoints,
   scanner/build checks, and DB assertions in `CollectResourceGateScenario`.
5. Use `given_*` methods for blocked collect and resource delegation facts.
6. Use `when_*` methods for resource result ACK, receipt upload, and scanner
   rounds.
7. Use `then_*` methods for scanner labels, gate release, no-release, platform
   dependency, and build eligibility assertions.
8. Keep SQL setup and scanner/build plumbing below the scenario layer in
   `collect_resource_gate/support.rs`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Collect TX ACK idempotency, integration:
  ACK side effect is sent once and durable ACK facts prevent duplicate sends.
- Collect receipt upload retry, integration:
  backend upload failure leaves receipt facts retryable.
- Collect notification success path, integration:
  a fresh collect order persists once and emits the expected notification.
- Move payload-only component tests closer to the receipt payload builder source
  once the source-side test layout is standardized.
