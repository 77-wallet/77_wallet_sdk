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

### Collect Scanner Does Not Own Local Reclaim

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_local_reclaim/mod.rs
  collect_shadow_scanner_no_longer_owns_local_undelegation_intents
  ```

- Backend: none.
- Scanner: collect scanner does not emit `ExecuteLocalUndelegation` or
  `RecoverLocalUndelegation`.
- DB facts: local undelegation task is present in the local transaction DB.
- Invariant: collect shadow scanner must not own local undelegation execution or
  recovery after those intents move to the local reclaim scanner.

### Local Reclaim Scanner Owns Local Undelegation

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_local_reclaim/mod.rs
  local_reclaim_shadow_scanner_owns_local_undelegation_intents
  ```

- Backend: none.
- Scanner: local reclaim scanner emits both `ExecuteLocalUndelegation` and
  `RecoverLocalUndelegation`.
- DB facts: one local undelegation task is build-ready, and one has broadcast
  success facts for recovery.
- Invariant: local reclaim scanner must be the owner for local undelegation
  execution and recovery intents.

### Collect Fee Cycle Skips Stale Rows

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_fee_cycle/mod.rs
  collect_scanner_skips_stale_fee_cycle_rows
  ```

- Backend: none.
- Scanner: emits no collect intent.
- DB facts: `need_service_fee = true`, `service_fee_uploaded_at` remains set,
  and `raw_tx` / `tx_hash` remain empty.
- Invariant: stale fee-cycle residue must not re-enter build or fee-result ACK
  scanning.

### Collect Fee Cycle Uploads Service Fee

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_fee_cycle/mod.rs
  collect_scanner_emits_upload_service_fee_when_need_service_fee_is_true
  ```

- Backend: none.
- Scanner: emits `UploadServiceFee` and does not emit `BuildTx`.
- DB facts: `need_service_fee = true`; service-fee order and upload timestamps
  remain empty until the upload step runs.
- Invariant: fee upload gating must run before collect build when service fee is
  still required.

### Collect Fee Cycle Reopen Builds Without Upload

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_fee_cycle/mod.rs
  collect_scanner_builds_after_fee_cycle_reopen_without_service_fee_upload
  ```

- Backend: none.
- Scanner: emits `BuildTx` and does not emit `SendTxFeeResAck`.
- DB facts: `need_service_fee = false`, `service_fee_uploaded_at = NULL`,
  `tx_fee_res_ack_sent_at = NULL`, and execution facts remain empty.
- Invariant: a reopened fee cycle without a real upload in the current cycle
  must not be blocked by historical ACK residue.

### Collect Fee Cycle ACKs Before Build

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_fee_cycle/mod.rs
  collect_scanner_emits_tx_fee_res_ack_before_build_after_fee_result
  ```

- Backend: none.
- Scanner: emits `SendTxFeeResAck` and does not emit `BuildTx`.
- DB facts: `need_service_fee = false`, `tx_fee_res_ack_sent_at = NULL`, and
  `raw_tx` remains empty.
- Invariant: fee-result ACK must be sent before build is allowed after a
  completed fee upload.

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

### Collect Blockhash Rebuild Clears Stale Facts

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_recovery/mod.rs
  collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr
  ```

- Backend: none.
- DB facts: stale `raw_tx` and `tx_hash` are cleared before rebuild; persisted
  `to_addr` is updated only by the next build step.
- Invariant: rebuilding after an expired blockhash must not keep stale execution
  facts or silently invent a replacement execution address.

### Collect Recover Queries Chain Before Rebuild

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_recovery/mod.rs
  collect_recover_queries_chain_before_any_expired_raw_rebuild_invalidation
  ```

- Backend: none.
- Chain: fake TRON adapter is queried once before any rebuild invalidation.
- DB facts: `transaction_time` is persisted, `last_broadcast_at` remains, and
  `raw_tx` is not cleared before the visible chain confirmation is handled.
- Invariant: recovery must prefer durable chain evidence over rebuilding an
  expired raw transaction too early.

### Collect Recover Backfills Missing Hash

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_recovery/mod.rs
  collect_recover_backfills_missing_tx_hash_before_receipt_upload
  ```

- Backend: none.
- Chain: fake TRON adapter confirms the recovered transaction.
- DB facts: recovered `tx_hash` is restored, `transaction_time` is persisted,
  and the row becomes eligible for execution-receipt upload.
- Invariant: receipt upload readiness must not depend on a transient missing
  local hash when the chain result can provide it.

### Collect Scanner Recovers Broadcast Visible Pending

- Layer: component.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_recovery/mod.rs
  collect_scanner_recovers_broadcast_visible_pending_result
  ```

- Backend: none.
- Scanner: emits `RecoverTx`, and does not emit `BuildTx` or
  `UploadServiceFee`.
- DB facts: pending row keeps `tx_hash`; `transaction_time` remains empty until
  recovery confirms the broadcast.
- Invariant: scanner routing must recover broadcast-visible pending rows instead
  of re-entering build or fee upload.

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

`collect_local_reclaim/mod.rs` uses a local scanner ownership contract:

1. Keep scanner ownership cases read-first in `collect_local_reclaim/mod.rs`.
2. Keep local SQLite setup and local undelegation seed data in
   `LocalReclaimScannerDb`.
3. Use `given_*` methods for local undelegation setup and broadcast-success
   recovery setup.
4. Use `when_*` methods for collect scanner and local reclaim scanner rounds.
5. Use `then_*` methods for scanner label ownership assertions.
6. Keep repository setup and scanner helper calls below the scenario layer in
   `collect_local_reclaim/support.rs`.

`collect_fee_cycle/mod.rs` uses a local scanner routing contract:

1. Keep fee-cycle scanner cases read-first in `collect_fee_cycle/mod.rs`.
2. Keep unique collect trade data in `CollectFeeCycleFixture`.
3. Keep local SQLite setup, collect seed data, fee-cycle SQL facts, scanner
   calls, and DB reloads in `LocalCollectFeeCycleDb`.
4. Use `given_*` methods for stale, waiting, reopened, and completed fee-cycle
   facts.
5. Use `when_*` methods for collect scanner rounds.
6. Use `then_*` methods for scanner label routing and persisted DB fact
   assertions.
7. Keep SQL setup and scanner helper calls below the scenario layer in
   `collect_fee_cycle/support.rs`.

`collect_recovery/mod.rs` uses a mixed-layer recovery contract:

1. Keep recovery test cases read-first in `collect_recovery/mod.rs`.
2. Keep unique trade and hash data in `CollectRecoveryFixture`.
3. Keep local SQLite-only recovery checks in `LocalCollectRecoveryDb`.
4. Keep shadow-worker recovery setup, fake chain probes, worker entrypoint, and
   DB assertions in `ShadowCollectRecoveryScenario`.
5. Use `given_*` methods for persisted stale facts, recoverable collect rows,
   and fake chain evidence.
6. Use `when_*` methods for invalidation, rebuild persistence, scanner rounds,
   and recover command execution.
7. Use `then_*` methods for cleared facts, scanner labels, chain query counts,
   hash backfill, and receipt-upload readiness.
8. Keep raw SQL setup, TRON probe adapter, raw transaction JSON, scanner helper,
   and worker construction below the scenario layer in
   `collect_recovery/support.rs`.

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
