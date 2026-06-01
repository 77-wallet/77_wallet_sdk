# API Wallet Assets Assertion Matrix

Scope: `wallet-api` API wallet asset sync and account-change flows.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Account Change Syncs API Wallet Token By Address

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/acct_change/mod.rs
  acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address
  ```

- Backend: none; the MQTT task is executed locally through the testkit.
- DB facts: API wallet SOL USDC coin and asset are seeded by token address.
- Payload: account-change symbol is lower-case `usd coin`, while the persisted
  asset symbol is `USDC`.
- Invariant: API wallet asset sync must match by chain and token address before
  symbol text, preserving the canonical `USDC` symbol.

### Account Change Syncs Normal Token By Address

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/acct_change/mod.rs
  acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch
  ```

- Backend: none; the MQTT task is executed locally through the testkit.
- DB facts: normal-wallet ETH USDT asset exists for the target address and token.
- Payload: account-change symbol is lower-case `tether usd`, while the persisted
  asset symbol is `USDT`.
- Invariant: normal wallet asset sync must match the existing token asset by
  token address and keep the canonical persisted symbol.

### Account Change Syncs Native By Empty Token

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/acct_change/mod.rs
  acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing
  ```

- Backend: none; the MQTT task is executed locally through the testkit.
- DB facts: normal-wallet native ETH asset exists with an empty token key.
- Payload: account-change omits `token` and provides symbol text `ether`.
- Invariant: native asset sync must match the existing empty-token native asset
  and keep the canonical `ETH` symbol.

## Template Contract

`acct_change/mod.rs` uses a read-first integration contract:

1. Keep account-change asset sync cases in `acct_change/mod.rs`.
2. Keep manager initialization, active-chain setup, seeded asset facts, MQTT
   payload construction, task-status polling, and final DB assertions in
   `AcctChangeScenario`.
3. Use `given_*` methods for API-wallet token assets, normal-wallet token
   assets, and normal-wallet native assets.
4. Use `when_*` methods for local MQTT account-change execution.
5. Use `then_*` methods for task success and canonical asset symbol assertions.
6. Keep repository setup, payload JSON, and task queue polling below the
   scenario layer in `acct_change/support.rs`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Account-change failure path:
  invalid or inactive chain data should not mutate unrelated asset rows.
- Asset sync wallet filtering:
  subaccount and API wallet asset sync rules should be documented separately
  when `sync.rs` is standardized.
