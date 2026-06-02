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

### Sync API Wallet Updates Asset From Chain

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/sync/mod.rs
  sync_api_assets_by_wallet_updates_api_assets_from_chain
  ```

- Backend: none; the chain balance path uses a fake BNB transaction adapter.
- DB facts: withdrawal wallet, initialized BNB account, native BNB coin, and
  API asset are seeded with balance `0`.
- Chain calls: fake adapter balance query is called once and returns `123`.
- Invariant: API wallet asset sync must persist the formatted on-chain balance
  for a withdrawal wallet.

### Sync API Wallet Keeps Balance On Chain Failure

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/sync/mod.rs
  sync_api_assets_by_wallet_keeps_balance_when_chain_query_fails
  ```

- Backend: none; the chain balance path uses a fake BNB transaction adapter that
  fails the balance query.
- DB facts: seeded asset balance remains `0`.
- Chain calls: fake adapter balance query is called once.
- Invariant: a chain balance failure must not clear or overwrite the existing
  asset balance.

### Sync API Wallet Skips Subaccount Wallet

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/sync/mod.rs
  sync_api_assets_by_wallet_skips_subaccount_wallet
  ```

- Backend: none; a fake BNB transaction adapter is installed but should not be
  queried.
- DB facts: subaccount wallet, initialized BNB account, native BNB coin, and API
  asset are seeded with balance `0`.
- Chain calls: fake adapter balance query is not called.
- Invariant: API wallet asset sync must skip subaccount wallets and leave their
  balances untouched.

## Template Contract

`acct_change/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep account-change asset sync cases in `acct_change/mod.rs`.
2. Keep manager initialization in `AcctChangeScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep API-wallet token assets, normal-wallet token assets, and normal-wallet
   native assets in the flow-local `AcctChangeGiven` trait.
5. Keep local MQTT account-change execution in the flow-local
   `AcctChangeWhen` trait.
6. Keep task success and canonical asset symbol assertions in the flow-local
   `AcctChangeThen` trait.
7. Keep seed/assert internals behind harness role containers and below the
   Given-When-Then test body.
8. Keep active-chain setup, seeded asset facts, repository setup, payload JSON,
   DB assertions, and task queue polling below the scenario layer in
   `acct_change/support`.

`sync/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep API wallet asset sync cases in `sync/mod.rs`.
2. Keep fake chain adapter state in `SyncAssetsScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep seeded wallet assets and fake chain balance behavior in the flow-local
   `SyncAssetsGiven` trait.
5. Keep `sync_api_assets_by_wallet` entrypoints in the flow-local
   `SyncAssetsWhen` trait.
6. Keep returned result, chain-call counts, and persisted asset balance
   assertions in the flow-local `SyncAssetsThen` trait.
7. Keep seed/load/count/assert internals behind harness role containers and
   below the Given-When-Then test body.
8. Keep adapter override and repository setup details below the scenario layer
   in `sync/support`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Account-change failure path:
  invalid or inactive chain data should not mutate unrelated asset rows.
- Asset sync token filtering:
  explicit symbol filters should be covered once the filter contract is
  standardized.
