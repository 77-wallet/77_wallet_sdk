# Ordinary coin metadata timestamps

| Scenario | Invariant | Component regression |
| --- | --- | --- |
| Insert distinct server timestamps | Creation and update times persist independently | `catalog_insert_preserves_distinct_server_timestamps` |
| Existing coin becomes default | Default and update time change; creation time and enabled status remain | `catalog_conflict_updates_default_and_server_update_time` |
| Rollback then retry | Rollback retains metadata; successful retry applies the received update | `catalog_rollback_preserves_default_and_timestamps` |
| Replay a legacy row with wrong update time twice | Timestamp/default repair without duplicate rows; creation time remains | `catalog_replay_repairs_legacy_timestamp_without_duplicate_rows` |

Run `cargo test -p wallet-database --offline --locked --lib catalog_metadata_tests -- --test-threads=1`.
Tests use temporary SQLite/migrations and the production repository/DAO, without backend, App data or RPC.

The production correction binds `CoinData.updated_at` to the ordinary coin DAO's update timestamp rather than binding creation time twice. Existing conflict update, transaction, precision reconciliation and last-writer behavior remain unchanged. Existing rows are corrected only when received again. Catalog filtering, synchronization watermarks, API-wallet DAO and actual device acceptance are outside this commit.
