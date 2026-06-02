# API Wallet Withdraw Assertion Matrix

Scope: `wallet-api` API wallet withdraw flow.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

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
