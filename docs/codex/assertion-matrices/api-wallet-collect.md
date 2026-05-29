# API Wallet Collect Assertion Matrix

Scope: `wallet-api` API wallet collect flow.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Collect Order Notification Retry

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/collect_notification.rs
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

## Template Contract

`collect_notification.rs` follows the API wallet integration scenario template:

1. Keep unique collect input data in `CollectOrderFixture`.
2. Keep environment, DB pool, notification setup, and collect actions in
   `CollectNotificationScenario`.
3. Arrange a persisted-failure case with a closed frontend notification sender.
4. Act through `api_collect_order`.
5. Assert the surfaced error, DB retry fact, and retry notification payload.

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
