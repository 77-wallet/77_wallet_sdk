# API Wallet Withdraw Assertion Matrix

Scope: `wallet-api` API wallet withdraw flow.

### Withdraw Dispatcher Watchdog Timing

- Layer: unit.
- Entrypoint:

  ```text
  src/infrastructure/api_trans/withdraw/shadow/dispatcher.rs
  queued_intent_is_not_watchdog_tracked_until_execution_starts
  ```

- Queue: an intent waiting for a dispatcher lane permit remains in duplicate
  suppression but has no watchdog start timestamp.
- Execution: watchdog timing begins only after the permit is acquired.
- Invariant: dropping the execution guard clears both tracking structures.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Withdraw Actual Resource Display

- Layer: unit.
- Entrypoint:

  ```text
  src/api/api_wallet/withdraw.rs
  api_withdraw_order_vo_fee_display_reads_legacy_energy_only_actual_resource
  ```

- DB facts: legacy `resource_consume` may be an energy-only scalar; current
  shadow rows persist JSON with bandwidth and energy.
- API facts: list/detail serialize `transactionFee`, `bandwidthConsume`, and
  `energyConsume` for successful orders.
- DB regression:

  ```text
  wallet-database/src/dao/api_withdraw.rs
  withdraw_resource_recovery_scans_scalar_success_and_updates_conditionally
  ```

- Invariant: completed orders never present estimated bandwidth as actual.
  Startup recovery keyset-pages through every scalar row once in batches of 20,
  serially backfills complete chain resource usage, and only updates an
  unchanged scalar value.

### Withdraw Order Notification Retry

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_notification/mod.rs
  withdraw_notification_retry_on_existing_trade_no
  ```

- Backend: one `TRANS_EVENT_ACK` with `type = WD`, `ackType = TX`
  after retry.
- DB facts: withdraw row keeps `init_status = AuditPass`; retry does not
  create duplicate TX ACK.
- Invariant: failed frontend notify must not lose the persisted withdraw order.

### Withdraw TX ACK

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_notification/mod.rs
  withdraw_tx_ack_sends_once_and_persists_fact
  ```

- Backend: exactly one `TRANS_EVENT_ACK` with `type = WD`,
  `ackType = TX`.
- DB facts: `tx_ack_sent_at` is set; scanner no longer emits `SendTxAck`.
- Invariant: repeated TX ACK worker execution must not send a second ACK.

### Withdraw TX ACK Backend Failure

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_notification/mod.rs
  withdraw_tx_ack_backend_failure_keeps_fact_unset_and_retryable
  ```

- Backend: one failed `TRANS_EVENT_ACK` attempt with `type = WD`,
  `ackType = TX`.
- DB facts: `tx_ack_sent_at` stays `NULL`; scanner still emits `SendTxAck`.
- Invariant: backend ACK failure must not persist the durable ACK fact.

### Withdraw Resource Result ACK Payload

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_resource_gate/mod.rs
  withdraw_resource_result_ack_uses_wd_rsc_dl_type
  ```

- Backend: one `TRANS_EVENT_ACK` with `type = WD_RSC_DL`,
  `ackType = TX_RES`.
- DB facts: resource delegation row remains addressable by
  `resource_trade_no`.
- Invariant: ACK payload must not use collect resource type.

### Withdraw Resource Gate Release

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_resource_gate/mod.rs
  withdraw_resource_result_ack_releases_origin_withdraw_gate
  ```

- Backend: resource result ACK is sent for the delegation trade.
- DB facts: `resource_gate_released_at` is set;
  `resource_gate_result = ResourceDelegationSuccess`; scanner emits `BuildTx`.
- Invariant: blocked withdraw must not build before the matching resource
  delegation succeeds.

### Withdraw Failed Resource Bypass

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_resource_gate/mod.rs
  withdraw_failed_resource_bypass_reopens_withdraw_build_flow
  ```

- Backend: resource execution receipt upload is attempted for the delegation.
- DB facts: `resource_gate_released_at` is set;
  `resource_gate_result = ResourceDelegationFailedBypass`; scanner emits
  `BuildTx`.
- Invariant: failed delegation bypass must release only the matching withdraw
  gate.

### Resource ACK Without Origin Trade

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_resource_gate/mod.rs
  withdraw_resource_result_ack_without_origin_trade_no_does_not_release_gate
  ```

- Backend: resource result ACK can be sent.
- DB facts: origin withdraw keeps `resource_gate_released_at = NULL`;
  `resource_gate_result = NULL`.
- Invariant: missing `origin_trade_no` must not release any withdraw gate.

### Resource ACK With Collect Origin Type

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/withdraw_resource_gate/mod.rs
  withdraw_resource_result_ack_for_collect_origin_does_not_release_withdraw_gate
  ```

- Backend: resource result ACK can be sent.
- DB facts: origin withdraw keeps `resource_gate_released_at = NULL`;
  `resource_gate_result = NULL`.
- Invariant: collect-origin delegation must not release withdraw gate.

### Withdraw Confirm Success

- Layer: component.
- Entrypoint:

  ```text
  src/domain/api_wallet/trans/withdraw.rs
  withdraw_confirm_success_writes_transaction_time_and_chain_success
  ```

- Backend: none.
- DB facts: `transaction_time` and `chain_success_at` are set;
  `chain_failed_at` stays empty; status becomes `Success`.
- Invariant: success confirm must not write failure fact.

### Withdraw Confirm Repeat Success

- Layer: component.
- Entrypoint:

  ```text
  src/domain/api_wallet/trans/withdraw.rs
  withdraw_confirm_repeat_success_does_not_notify_again
  ```

- Backend: none.
- DB facts: existing `transaction_time` and `chain_success_at` are preserved.
- Invariant: repeat confirm must not notify again or mutate success facts.

### Withdraw Confirm Failure

- Layer: component.
- Entrypoint:

  ```text
  src/domain/api_wallet/trans/withdraw.rs
  withdraw_confirm_failure_writes_transaction_time_and_chain_failed
  ```

- Backend: none.
- DB facts: `transaction_time` and `chain_failed_at` are set;
  `chain_success_at` stays empty; status becomes `Failure`.
- Invariant: failure confirm must not write success fact.

### Withdraw Confirm Missing Trade

- Layer: component.
- Entrypoint:

  ```text
  src/domain/api_wallet/trans/withdraw.rs
  withdraw_confirm_missing_trade_no_errors
  ```

- Backend: none.
- DB facts: no row is created.
- Invariant: pool seam must surface missing rows instead of silently
  succeeding.

### Withdraw Chain TX Actions Share Running Key

- Layer: component.
- Entrypoint:

  ```text
  src/infrastructure/api_trans/withdraw/shadow/dispatcher.rs
  chain_tx_actions_share_running_key
  ```

- Backend: none.
- Dispatcher facts: `BuildTx`, `BroadcastTx`, and `RecoverTx` for the same
  withdraw `trade_no` map to the same running key.
- Invariant: build, broadcast, and recover for one withdraw order must not run
  concurrently.

### Withdraw Expired TRON Raw Keeps TX Hash

- Layer: component.
- Entrypoint:

  ```text
  src/infrastructure/api_trans/withdraw/shadow/worker/shadow_withdraw_worker.rs
  withdraw_expired_tron_raw_with_tx_hash_is_not_invalidated_for_recover
  ```

- Backend: none.
- DB facts: persisted `tx_hash` is treated as possible broadcast evidence even
  when `last_broadcast_at` is still empty.
- Invariant: recover must not clear `raw_tx` / `tx_hash` for rebuild when the
  withdraw row already has a transaction hash.

### Withdraw Missing TRON Transaction Uses Uncertain Timestamp

- Layer: unit.
- Entrypoint:

  ```text
  src/domain/api_wallet/trans/shared_mechanism.rs
  tron_missing_confirmed_and_pending_retry_waits_for_timeout
  ```

- Backend: none.
- DB facts: a historical uncertain row may have no `last_broadcast_at` while
  retaining `broadcast_uncertain_since_at`.
- Invariant: Recover uses `last_broadcast_at` when present, otherwise falls back
  to `broadcast_uncertain_since_at`; with neither timestamp it must keep waiting.
- Worker behavior: after timeout, withdraw keeps its existing manual-review
  transition and does not adopt collect's rebroadcast policy.

### Withdraw Broadcast RPC Timeout Preserves Chain Facts

- Layer: unit / component.
- Entrypoint:

  ```text
  src/infrastructure/chain_rpc_guard.rs
  rpc_timeout_returns_elapsed_for_slow_future
  src/infrastructure/api_trans/withdraw/shadow/worker/shadow_withdraw_worker.rs
  ```

- Backend: none.
- Chain/RPC facts: guarded broadcast RPC await time is bounded by
  `CHAIN_RPC_GUARD_RPC_TIMEOUT_MS`.
- Invariant: broadcast timeout is treated as uncertain, preserving existing
  `tx_hash` / `raw_tx` so recover can query chain facts after the current
  `ChainTx` attempt exits.

### Withdraw Resource Gate Uses Independent Single Lane

- Layer: unit.
- Entrypoint:

  ```text
  src/infrastructure/api_trans/withdraw/shadow/dispatcher.rs
  resource_gate_uses_single_independent_dispatcher_lane
  ```

- Backend: none.
- Dispatcher facts: all `EvalResourceGate` intents use the process-wide shared
  one-permit lane; Build, Broadcast, and Recover continue using the chain lane.
- Invariant: an audit batch must not create concurrent TRON resource-gate RPC
  evaluations, overlap Collect resource evaluation, or consume the dispatcher
  capacity reserved for transaction execution and recovery.

### Withdraw Build Balance Failure

- Layer: component + worker unit.
- Entrypoint:

  ```text
  wallet-database/src/dao/api_withdraw.rs
  withdraw_build_failure_marks_error_and_blocks_can_build_scan
  withdraw_build_failure_blocks_resource_gate_scan

  wallet-api/src/infrastructure/api_trans/withdraw/shadow/worker/shadow_withdraw_worker.rs
  withdraw_build_failure_classifies_balance_errors_as_terminal
  ```

- DB facts: `tx_ack_sent_at`, `audit_passed_at`, and
  `resource_gate_released_at` are present; `raw_tx`, `transaction_time`,
  `finished_at`, and `err_code` are absent before the failure.
- Failure fact: deterministic BuildTx balance failures write `err_code=6001`,
  `failure_stage=Build`, clear `building_at`, and recompute status to
  `SendingTxFailed`.
- Invariant: after the failure fact is written, `scan_can_build` must not pick
  the withdraw order again; pre-Build resource-gate balance failures must also
  be excluded from `scan_need_resource_gate`; transient Build/RPC failures
  remain retryable.

### Withdraw Fee Estimate Fast Path

- Layer: scanner unit/component.
- Entrypoint:

  ```text
  src/infrastructure/api_trans/withdraw/shadow/scanner.rs
  withdraw_try_advance_dispatches_fee_estimate_before_audit
  ```

- DB facts: TRON withdraw, `fee_estimated_at IS NULL`, `raw_tx IS NULL`,
  `transaction_time IS NULL`, `finished_at IS NULL`, `err_code IS NULL`.
- Intent: point-to-point `try_advance` dispatches `EstimateFee` before TX ACK,
  resource gate, build, broadcast, or recover intents.
- Invariant: a newly submitted audit-pending withdraw order should get its
  display fee snapshot through the fast path instead of waiting for the next
  periodic scanner round.

## Template Contract

`withdraw_notification/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep local immutable data in a `*Fixture`.
2. Keep environment and DB pools in `WithdrawNotificationScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep wallet, withdraw order, notification-channel, and backend failure setup
   in the flow-local `WithdrawNotificationGiven` trait.
5. Keep initial submit, retry, and TX ACK worker execution in the flow-local
   `WithdrawNotificationWhen` trait.
6. Keep surfaced error, DB facts, backend TX ACK calls, scanner retry state,
   notification payload, and idempotency assertions in the flow-local
   `WithdrawNotificationThen` trait.
7. Keep seed/load/count/assert internals behind harness role containers and
   below the Given-When-Then test body.
8. Keep wallet seed, notification channel, DB pool, scanner labels, backend ACK
   counting, and payload decryption details below the scenario layer in
   `withdraw_notification/support`.

`withdraw_resource_gate/mod.rs` is the role-trait Given-When-Then template for
resource gate flows:

1. Keep the resource gate test cases in `withdraw_resource_gate/mod.rs`.
2. Keep unique trade data in `WithdrawResourceGateFixture`.
3. Keep environment and DB pools in `WithdrawResourceGateScenario`.
4. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first test roles using generic containers from `tests/harness`.
5. Keep blocked withdraw and resource delegation facts in the flow-local
   `WithdrawResourceGateGiven` trait.
6. Keep result ACK and receipt upload worker entrypoints in the flow-local
   `WithdrawResourceGateWhen` trait.
7. Keep backend payload, gate release, no-release, and scanner build
   eligibility assertions in the flow-local `WithdrawResourceGateThen` trait.
8. Keep seed/load/assert internals behind harness role containers and below
   the Given-When-Then test body.
9. Keep SQL setup below the scenario layer in
   `withdraw_resource_gate/support/db.rs`.
10. Keep backend payload wait/decrypt/assert details in
   `withdraw_resource_gate/support/assertions.rs`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- TX result ACK ordering, integration:
  `tx_res_ack_sent_at` stays `NULL` until `tx_res_received_at` exists.
- Withdraw receipt upload idempotency, integration:
  `tx_exec_receipt_uploaded_at` is written once; duplicate worker run makes
  no second upload.
- Concurrent TX ACK execution, integration:
  same `trade_no` can produce at most one backend ACK.
