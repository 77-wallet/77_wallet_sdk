# API Wallet Assets Assertion Matrix

Scope: `wallet-api` API wallet asset sync and account-change flows.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Protected API Wallet Asset Queries

- Layer: unit.
- Entrypoint:

  ```text
  src/infrastructure/protected_query.rs
  protected_query_deduplicates_concurrent_loads
  protected_query_returns_stale_on_timeout

  src/infrastructure/api_wallet_asset_query_guard.rs
  account_assets_key_keeps_query_dimensions
  asset_detail_key_keeps_query_dimensions

  src/domain/api_wallet/assets/total_query_policy.rs
  total_assets_dedup_allows_single_real_query
  total_assets_query_returns_stale_cache_on_timeout

  src/domain/api_wallet/wallet/wallet_list_policy.rs
  api_wallet_list_query_deduplicates_concurrent_loads
  api_wallet_list_query_returns_stale_on_timeout
  ```

- Backend: none; tests cover local query protection behavior only.
- DB facts: none.
- Invariant: API wallet asset read endpoints may share protection mechanics
  while retaining distinct cache keys and business query semantics. Transient
  timeout/pool-timeout failures may return stale cached values only when a prior
  successful value exists. API wallet list reads use the same protected-query
  behavior so repeated subaccount-list polling cannot fan out into duplicate
  list queries under batch collect load.

### Large-Wallet Total Reads Start From Nonzero Assets

- Layer: component.
- Entrypoint:

  ```text
  wallet-database/src/dao/api_assets.rs
  api_wallet_total_assets_v3_uses_sparse_nonzero_assets

  wallet-database/src/lib.rs
  api_wallet_migration_creates_nonzero_asset_total_index

  wallet-database/src/repositories/api_wallet/account.rs
  count_by_wallet_address_up_to_stops_at_limit

  wallet-api/src/domain/api_wallet/assets/total_query_policy.rs
  whole_wallet_count_is_bounded_but_filtered_counts_remain_exact
  ```

- Backend: none; tests use a migrated temporary SQLite database.
- DB facts: active and inactive accounts, active and inactive chains, zero,
  nonzero, disabled, and unrelated-wallet assets are seeded.
- Invariant: V3 wallet totals scan the partial nonzero-asset covering index and
  still return only active, nonzero assets belonging to the requested active
  wallet/account/chain. Zero-balance assets remain persisted and are not
  removed or skipped during creation. Whole-wallet V2/V3 routing scans at most
  threshold plus one active account, while account- or chain-filtered routing
  retains exact-count behavior.

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
  sync_api_assets_by_wallet_traverses_multiple_address_pages
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

### Sync API Wallet Bounds Withdrawal Address Memory

- Layer: component plus integration.
- Entrypoints:

  ```text
  wallet-database/src/repositories/api_wallet/account.rs
  account_repo_pages_active_wallet_addresses_by_stable_id_cursor

  tests/integration/api_wallet/sync/mod.rs
  sync_api_assets_by_wallet_updates_api_assets_from_chain
  ```

- Backend: none; the integration balance path uses a fake chain adapter across
  201 addresses, crossing the 200-address page boundary.
- DB facts: active API-account addresses are traversed by increasing row ID for
  one wallet; inactive accounts and accounts belonging to another wallet are
  excluded.
- Invariant: withdrawal-wallet synchronization loads and completes one bounded
  address page before reading the next page, while preserving the existing
  synchronous `Result<()>` API contract.
### Empty Withdrawal Wallet Never Falls Back To All Assets

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/sync/mod.rs
  sync_empty_withdrawal_wallet_does_not_sync_other_wallet_assets
  ```

- DB facts: one withdrawal wallet has no active API-account rows while another
  wallet owns an initialized account and native asset.
- Invariant: an empty address result must finish without calling a chain adapter
  and must never interpret an empty address vector as an unfiltered
  `api_assets` query.

### Backend Asset Query Dispatches Updated Token Events

- Layer: unit.
- Entrypoint:

  ```text
  src/infrastructure/api_wallet_assets_sync.rs
  api_asset_query_dispatches_updated_token_events
  ```

- Backend: none; the test covers the local dispatch-plan builder only.
- DB facts: none.
- Payload: a TRON address has a JST token update, with a duplicate JST row and a
  separate USDT row.
- Invariant: backend asset-list balance updates must schedule refresh work for
  the actual updated token key and address, and must not collapse contract-token
  updates into a native-token-only refresh.

### Account Change Persists API Asset Sync State

- Layer: component.
- Entrypoint:

  ```text
  wallet-database/src/repositories/api_wallet/api_asset_sync_state.rs
  api_asset_sync_state_upsert_and_claim_success
  api_asset_sync_state_claim_is_exclusive
  api_asset_sync_state_done_is_not_overwritten_by_failed_worker
  api_asset_sync_state_trims_done_rows_to_limit
  ```

- Backend: none; the tests use local SQLite state only.
- DB facts: API wallet asset-sync states are keyed by address, chain, and token
  address.
- Payload: a TRON JST address sync state is inserted and claimed by the recovery
  path.
- Invariant: account-change-triggered API wallet asset sync must be durable and
  claimable after restart, while concurrent workers cannot claim the same state
  twice. A successful inner-event balance write may mark the state done, and a
  later recovery-worker failure must not overwrite that clean state. Done states
  are bounded by retention and row-limit cleanup so the table cannot grow
  indefinitely.

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

## Asset sync resolves funded balance reminders (2026-10-03)

- Trigger: successful API withdrawal-wallet asset synchronization and persistence.
- Reuse formatted balances from that synchronization; do not query RPC again.
- Match reminder UID, address, chain, token; compare its current threshold and
  resolve using payload compare-and-set before the existing asset frontend event.
- Resolution preserves unread/read state and unrelated reminders.
- Below-threshold, invalid, failed-query and unpersisted observations stay active.
- Existing five-second banner buffer cannot hide a resolved reminder.
- Integration regressions: `sync_api_assets_resolves_funded_reminder_without_another_balance_query`,
  `sync_api_assets_keeps_reminder_when_balance_is_insufficient`,
  `sync_api_assets_keeps_reminder_when_balance_query_fails`.
- Component regressions: `synced_balance_removes_only_matching_reminder_inside_banner_buffer`,
  `synced_balance_keeps_reminder_for_insufficient_invalid_or_unpersisted_balance`.
- Fake chain plus temporary SQLite only; frontend page/navigation and live
  transaction delivery are outside this offline verification.

### 2026-10-06 Global withdrawal banner query
- Current API pool active Withdrawal UIDs only (DISTINCT); exclude subaccounts, inactive wallets, empty UID and orphaned notification/order history. No seed/address-account bulk scan.
- `get_api_wallet_banners()` accepts no UID; message-center entrypoints keep UID. Both groups retain count/latestAt/items across eligible owners.
- Component regression: `global_banners_include_only_active_withdrawal_owners_and_keep_failures` covers empty scope, duplicate owner, two eligible wallets, funded resolution, failed RPC retention, untouched unread state, buffered reread, and all wallets disabled. Existing per-UID/CAS/synced-balance tests remain.
- Final verification: business-notification lib tests 11/11 and asset-sync integration 8/8 pass offline with wallet-api test debug=0/incremental=false; rustfmt/check-diff pass. Independent SDK and generated-binding review found no important findings. First full-debug compile was stopped for disk capacity, then compact validation completed; no cleanup. No new controlled concurrency/DB failure test; prior per-UID behavior reused.

### 2026-10-06 Independent business queries (supersedes combined banner contract)
- `get_pending_api_withdrawals()` lives in withdraw and returns existing order records; `get_unresolved_api_address_balance_notifications()` lives in business notifications and returns existing notification records. Neither accepts UID; no SDK UI grouping/count/latestAt.
- Reuse DISTINCT active Withdrawal UID selection, exclude subaccounts/disabled/orphan history. Pending predicate: Init and neither approved nor rejected; no balance lock/RPC dependency.
- `global_business_queries_scope_cache_and_failure_independence` uses isolated SQLite to cover multi-owner aggregation/deduplication, exclusion, perUID buffer, controlled held balance lock, and independent errors from each underlying table. Existing exact payload CAS, failed/timeout checks, synchronized asset resolution and UID message-center tests remain.
- Prior combined-interface evidence and review do not verify this revision. New results are recorded in the split implementation freeze evidence; independent review is pending parent coordination.

- Timeout coverage: `current_balance_resolves_only_after_successful_threshold_check` now also holds the injected RPC pending and verifies the existing three-second timeout preserves the unresolved row and read state; only this affected test is rerun after adding assertions. Production code remains identical to the 11-test and 8-sync-test checkpoints.

### 2026-10-06 Local reads and directed checks (supersedes synchronous balance refresh)

- Both queries complete while the observation mutex and existing background permits are held; local database failures remain independent.
- Directed checks work without an asset/account row, coalesce identical UID/id/payload observations for five seconds, and skip obsolete payloads.
- Exact replay preserves resolved/read state; changed payload reopens. Startup seed scopes active Withdrawal owners and delivery task IDs deduplicate.
- Resolution write failures retain the notification and persist compensation in the task database; cached successful observations can retry the failed CAS. RPC failure/timeout retains rows.
- Backend serialization strictly emits targetAddress/waddressType; no aliases or historical-data migration.
- Fresh verification is recorded under target/local-business-*.log; prior synchronous-query checkpoints do not validate this revision.

### 2026-10-06 Approved readiness wait-order regression

- Existing monotonic AtomicBool/Notify contract retained. Create Notified before the Acquire readiness check so a Release publication plus notify_waiters in the false-check/first-poll gap is observed. No new lifecycle, retry, global mutex or scheduler.
- Local-state unit tests cover that controlled gap, already-ready bypass, and one broadcast releasing multiple waiters. Old wait order must time out at the controlled gap; corrected order must complete.
- Standalone harness imports the actual source module against locked Tokio1.52.1. It verifies this module only; project test coverage is reported separately under target/readiness-validation.

### 2026-10-06 Notification production SQL moved to existing DAO

- Seven repository methods/eight SQL statements delegate to ApiBusinessNotificationDao; SQL text, bind order, deduplication and exact UID/id/payload CAS remain unchanged. Repo retains read/write pool selection, offset arithmetic and Pagination assembly. No schema change.
- SQLite regression pagination_count_and_unresolved_filter_remain_uid_scoped covers owner-only count, filter-before-offset, empty pages, resolved history and read-state independence; existing six notification tests cover durable reopening/replay, UID isolation, read/resolve state and stale-payload CAS.
- Current starting index captured separately because previously frozen files were staged by an unknown actor without content changes; migration preserves that index. Validation and new full freeze recorded under target/notification-dao-validation and target/notification-dao-review-snapshot.

### Test-only isolation and legacy live replay conditions

The three wallet session tests run their complete fixture in separate child processes because Context and unlock state are process-global. Balance coalescing uses one shared fake-check counter; either overlapping caller may win, and exactly one check must execute.

The legacy account-change and address-expansion replays now reside in `tests/smoke/api_wallet/mqtt_replay.rs` and are explicitly ignored by default. They initialize the real configured client and may update backend wallet/account state. Run only with separately authorized development/test access, a matching local `examples/config.yaml`, test device/account/wallet fixtures matching the payload UIDs, and no production configuration. They are not offline regressions. Manual command after those prerequisites: `cargo test -p wallet-api --test mod smoke::api_wallet::mqtt_replay:: -- --ignored --test-threads=1`.

Collect/fee missing rows are idempotent success; SQL/table failures remain database errors. The MQTT resource-result regression asserts the gate remains blocked before result ACK; the existing fake-backend resource-gate integration tests cover ACK release and invalid origins.


## Balance observation event snapshot

- Before chain RPC, snapshot eligible notification event IDs and payloads for the selected assets.
- After successful asset writes, only this snapshot may be reconciled: the latest valid event controls each group threshold, and success atomically resolves its observed earlier members with version guards; read/history remain unchanged.
- A snapshot read failure does not stop asset sync; it safely skips automatic reminder resolution.
- Component regression: `business_notification::event_history_tests::asset_sync_observation_cannot_resolve_event_arriving_during_rpc` verifies that a during-RPC arrival is neither resolved nor cached by the older observation.

2026-10-07 product revision: snapshot includes resolved history to prevent fallback to old thresholds; latest resolved suppresses the group. `asset_sync_latest_lower_threshold_resolves_observed_older_high_event` covers downward threshold changes, and existing during-RPC low-threshold arrival remains unresolved and uncached.

### EVM Contract Case Compatibility (2026-10-09)

- Scope: Polygon USDT account-change asset refresh and the shared token identity rule; no live DB migration or nonce changes.
- Unit: `token_case_canonicalizes_only_supported_evm_contracts`, `token_case_mixed_mqtt_selects_existing_polygon_asset`, `token_case_non_evm_remains_case_sensitive`, `token_case_normal_wallet_selects_evm_only`, `token_case_evm_detail_cache_keys_share_identity`.
- Component: `token_case_legacy_asset_lookup_and_balance_update`, `token_case_mixed_request_updates_lowercase_row`, `token_case_coin_join_reads_legacy_spelling`, `token_case_duplicate_rows_prefer_exact_detail_without_merging`, `token_case_recovery_keeps_mixed_case_evm_task`. Migrated temporary SQLite only, no backend or RPC.
- Expected: mixed MQTT token matches existing lower-case Polygon USDT; canonical event/cache keys are lower-case for the supported EVM routes only. Legacy stored spellings remain readable and updateable without inserting or migrating records. Existing coin metadata is reused despite EVM casing differences. Recovery existence does not discard a task solely because its token differs in case.
- Invariants: non-EVM case sensitivity, native identity, wallet/chain isolation, original transaction boundaries and status/coin/chain filters. Repeat balance writes keep one existing row. If historical duplicate rows already exist, detail prefers its exact key; this change does not merge/delete them or claim aggregate de-duplication.
- Red evidence: initial three DB regressions failed against baseline; after the first fix those passed while the exact-key duplicate guard failed. API red and final green evidence are recorded in PLANS.md.
- Validation commands: `cargo test --offline --locked --target-dir target -p wallet-database --lib token_case`; `cargo test --offline --locked --target-dir target -p wallet-api --no-default-features --lib token_case`; affected repository, sync-filter and cache/recovery groups.
- Limitation: selected boundaries are tested independently; no funded/live transfer, installed-App SDK replacement, or full MQTT-to-fake-RPC end-to-end acceptance in this batch. Out-of-order balance-response policy is unchanged.

### 2026-10-13 API EVM token identity and existing duplicate repair

Supersedes the earlier token-case batch's deliberate preservation of duplicate records, following the user's explicit request to fix coin sync/startup supplementation and clean existing data. Ordinary-wallet tables and transaction tables remain outside this batch.

- Coin DAO writes canonicalize supported EVM contract keys and deduplicate one input batch; asset metadata/default/balance upserts share that key. Startup supplementation normalizes existing keys and prevents duplicate coins within a page. Canonical read filters still accept checksum requests, including chain-list detail metadata.
- User confirms this new-chain rollout currently has only their testing device. Existing duplicates were cleaned once on that simulator. User additionally requests removing the remaining old-key UPDATEs. Startup migration now only creates two EVM-only unique indexes; no temporary tables, data conversion/deletion or sync-state changes. This revision supports the already-cleaned test DB and fresh DBs; unnormalized older DBs, including ETH checksum keys, need separate preparation before adoption. Non-EVM tokens keep case and native keys stay empty.
- DB regression group evm_token_identity covers index creation rollback, bypass duplicate rejection, mixed-case coin/default/balance writes, checksum query/price/delete, batch return identity, native/non-EVM and sync-state/balance invariance including repeated migration, and existing contract keys staying untouched by schema migration. Historical cleanup tests were removed with the retired production cleanup logic. Existing token_case fixtures inject a single raw legacy spelling explicitly; repeated metadata writes assert one preserved snapshot.
- API regressions evm_token_identity_backfill_reuses_legacy_key_and_deduplicates_batch / backfill_keeps_non_evm_case_distinct / chain_requests_match_canonical_metadata pin the changed boundaries.
- Simulator repair uses a consistent SQLite backup and one transaction against the running API DB; it omits new unique indexes until the installed App contains the new DAO writers. Old binaries can recreate duplicates until SDK/plugin/App rebuild. No private keys or transaction DB edits.
- Necessary command evidence and fixed-version review are recorded in PLANS.md and target/usdt-investigation. No 500k-address performance validation, network freshness/order changes, funded transactions or installed-new-SDK claim.
