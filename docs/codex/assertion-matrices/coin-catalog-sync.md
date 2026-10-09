# Ordinary coin timestamps and incremental sync

The independent timestamp correction is committed as37eb285b. Its four temporary SQLite regressions cover distinct insert timestamps, conflict defaults/update time, rollback/retry and repeated legacy-row correction; use cargo test -p wallet-database --offline --locked --lib catalog_metadata_tests -- --test-threads=1.

## Uncommitted incremental candidate

| Scenario | Required invariant | Fake-backend integration regression |
| --- | --- | --- |
| Existing501-row catalog, future update to old USDT plus newly created USDC; repeat | Separate bounded filters from latest creation baseline, repaired default/time, no duplicate identities, overlap includes last successful second | ordinary_catalog_incremental_fetches_future_old_updates_and_new_coins_above_500 |
| Historical row not updated again | No full/backfill request; old metadata remains until a future update arrives | ordinary_catalog_does_not_backfill_history_but_fetches_later_updates |
| Last update page fails/cancels | No partial metadata/assets, frozen starting cursor retained; retry begins page0 | ordinary_default_assets_failed_later_page_writes_no_partial_assets_or_coins; ordinary_default_assets_cancelled_last_page_does_not_write_or_notify |
| First request or creation traversal fails | Both traversals must complete before upsert/progress advancement | ordinary_catalog_request_and_creation_route_failures_do_not_advance |
| Metadata transaction or progress-save fails | Metadata rollback or safe replay; cursor never advances past failed work | ordinary_catalog_metadata_and_progress_failures_replay_old_window |
| Two simultaneous invocations | Second query waits for first completion and reads the resulting cursor | ordinary_catalog_serializes_overlapping_syncs_before_cursor_read |
| Same coin returned with different versions from both routes | Newer update version wins, one row persists, creation/update watermarks stay independent | ordinary_catalog_overlapping_queries_keep_newest_server_version |
| Invalid server timestamp then retry | No metadata write or progress advancement until valid response | ordinary_catalog_invalid_server_time_preserves_cursor_until_retry |
| New-install initialization paused at last page | Existing unfiltered initialization retained; no metadata/assets until all pages succeed | ordinary_default_assets_wait_for_last_page_then_fill_and_repeat_safely |
| Later completion fails after metadata | Null new-install starting state retained; retry completes initialization/reconciliation | ordinary_default_assets_failed_catalog_completion_defers_fill_until_retry |

Run correct default features: cargo test -p wallet-api --offline --locked --test mod ordinary_default_assets -- --test-threads=1. Tests use only temporary SQLite and localhost fake backend. They assert actual requests, persisted coin/default timestamps, assets, progress and failure/retry behavior. Existing cleanup before network and notification warning semantics are not redefined. Server offset-page drift, long-backdated updates, cross-process synchronization, real device acceptance and catalog-scale timing remain unverified; two-second overlap is not an absolute completeness claim.

| Persistence scenario | Evidence |
| --- | --- |
| Close all old SQLite read/write pools and replace Context on same files | `ordinary_catalog_rebuilt_context_restores_distinct_successful_watermarks`: distinct persisted watermarks drive exact independent cutoffs |
| Frozen initial state survives replacement after network failure | `ordinary_catalog_rebuilt_context_retries_frozen_baseline_after_network_failure`: initial window reused despite poisoned local updated_at |
| Successful initialization is not repeated after replacement | `ordinary_catalog_rebuilt_context_avoids_reinitialization_and_fetches_future_updates`: incremental requests apply later updates |
| Old Rust process exits before a new process opens identical files | `ordinary_catalog_process_exit_restores_persisted_watermarks`: success and first-request-failure pairs, child execution count and exact cutoffs asserted |

Context replacement keeps the original static allocation leaked to avoid unsafe reclamation; all old DB pools are explicitly closed. Separate-process coverage supplies process teardown evidence. Neither is a real App restart test.
