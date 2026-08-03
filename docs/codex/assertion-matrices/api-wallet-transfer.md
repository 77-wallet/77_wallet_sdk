# API Wallet Transfer Assertion Matrix

Scope: `wallet-api` API wallet transfer flow.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Transfer Nonce Lock Serializes Same Address

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/transfer_nonce/mod.rs
  api_wallet_transfer_nonce_lock_keeps_same_address_requests_serial
  ```

- Backend: none; the chain path uses a fake BNB transaction adapter.
- DB facts: BNB nonce floor starts at `0` for the fixture wallet.
- Chain calls: first transfer records nonce `1`; second transfer stays blocked
  until the first transfer is released, then records nonce `2`.
- Result: returned transaction hashes encode nonce `1` and nonce `2`.
- Invariant: transfers from the same address must reserve and use nonces
  serially under concurrency.

### Transfer Failure Keeps Reserved Nonce

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/transfer_nonce/mod.rs
  api_wallet_transfer_nonce_failure_keeps_reserved_nonce
  ```

- Backend: none; the chain path uses a fake BNB transaction adapter that fails
  transfer.
- DB facts: after the simulated transfer failure, nonce floor remains advanced
  to `1`.
- Chain calls: fake adapter records exactly nonce `1`.
- Invariant: a failed transfer must not rewind the reserved nonce and make it
  reusable.

## Template Contract

`transfer_nonce/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep transfer nonce cases in `transfer_nonce/mod.rs`.
2. Keep fake chain adapter state, fixture address, and adapter guard in
   `TransferNonceScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep BNB fixture setup, fake chain adapter behavior, and cached wallet
   password in the flow-local `TransferNonceGiven` trait.
5. Keep transfer entrypoints and explicit release of the blocked first transfer
   in the flow-local `TransferNonceWhen` trait.
6. Keep recorded nonce order, result hashes, error text, and persisted nonce
   floor assertions in the flow-local `TransferNonceThen` trait.
7. Keep seed/load/assert internals behind harness role containers and below the
   Given-When-Then test body.
8. Keep adapter override, repository setup, and transfer request construction
   details below the scenario layer in `transfer_nonce/support`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Transfer nonce multi-address concurrency:
  different sender addresses should not block each other.
- Transfer idempotency:
  retry behavior should not double-send a transaction after a successful chain
  response.
