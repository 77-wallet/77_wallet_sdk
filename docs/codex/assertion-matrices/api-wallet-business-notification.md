# API Wallet Business Reminder Assertions

## Scope

The `api_business_notification` table is in `api_wallet.db`. It stores API wallet
message-center reminders such as outgoing address balance insufficiency. Withdrawal
review remains in the withdrawal order and review list; it does not create a row here.

## Durable Facts and Invariants

| Fact or action | Invariant | Test entrypoint |
| --- | --- | --- |
| API wallet migration | The table exists in `api_wallet.db`, never in `data.db`. | `wallet_database::repositories::api_business_notification::tests::core_database_does_not_create_api_business_notification_table`; `notification_survives_reopen_and_is_scoped_by_uid` |
| UID-scoped reads and updates | A UID cannot list or mark another UID's reminder read. | `wallet_database::repositories::api_business_notification::tests::duplicate_delivery_preserves_read_and_resolution_state`; `wallet_api::service::api_wallet::business_notification::tests::balance_reminder_list_and_read_are_uid_scoped` |
| Duplicate delivery | A duplicate business key updates the payload but preserves read and resolved facts. | `wallet_database::repositories::api_business_notification::tests::duplicate_delivery_preserves_read_and_resolution_state` |
| Read and resolution | Reading a reminder does not resolve its active condition; resolved reminders remain in history. | `wallet_api::service::api_wallet::business_notification::tests::balance_reminder_list_and_read_are_uid_scoped`; `wallet_database::repositories::api_business_notification::tests::duplicate_delivery_preserves_read_and_resolution_state` |

MQTT ingestion and fresh-balance resolution are separate stages, pending the final
backend body contract. No external backend or chain calls occur in the tests above.

## Balance MQTT envelope parsing

`AwmAddressInsufficient::from_message` parses the backend's known fields and the
assumed `chainCode`/`address` additions. It copies the outer UID into the serializable
task body before queuing. It rejects missing UID, empty address or chain, and a
non-API-withdrawal wallet type. The focused `address_insufficient::tests` exercise
numeric/string decimal values, replay serialization, and invalid identity. The
backend's final spelling of the two added fields remains to be confirmed.

## Durable task name

The `AwmAddressInsufficient` task name is recognized when read back from SQLite.
The focused `address_insufficient_task_name_survives_sqlite_round_trip` test binds
the known task name and asserts that SQLite decoding preserves the known variant.

## MQTT reminder ingestion

`AWM_ADDRESS_INSUFFICIENT` is routed from the ordinary wallet MQTT topic to a
durable task. The queued body includes the outer UID. Executing the task writes
to `api_wallet.db` using UID, chain, address, and token as the stable alert key.
The focused `repeated_alert_updates_one_uid_scoped_reminder` test checks that a
repeat event updates the displayed remaining balance and reopens a resolved alert
without creating a second reminder or exposing it to another UID. The App banner
is a subsequent batch.

## Balance-based resolution

The unresolved reminder query refreshes each balance alert using the existing API
wallet chain-balance service. It resolves a row when the fresh balance reaches its
stored `thresholdValue`. Query or parse failures leave the row active. The saved
`remaining` display snapshot is unchanged. The focused
`current_balance_resolves_only_after_successful_threshold_check` test covers query
failure, below-threshold balance, exact-threshold balance, and snapshot retention.

## Unresolved Reminder Pagination Regression

- Layer: component; local SQLite and injected balance responses, no network.
- Entrypoint: `wallet-api/src/service/api_wallet/business_notification.rs` / `unresolved_page_checks_items_that_shift_in_after_resolution`.
- Invariant: all initially unresolved reminders for the UID are checked before the final page and `totalCount` are read. A resolved first-page item cannot leave a newly shifted item unchecked.
- Failure behavior: balance query failure keeps the reminder active (covered by `current_balance_resolves_only_after_successful_threshold_check`).

## Message-center unread count and all-read

- Layer: component; temporary API-wallet SQLite, no external network.
- Entrypoint: `wallet_database::repositories::api_business_notification::tests::unread_count_and_mark_all_read_cover_history_and_preserve_uid_isolation`.
- Unread count includes resolved history and records beyond the first page.
- Mark-all-read applies only to the requested UID and can be repeated safely.
- An unknown UID changes no records. Another UID remains unread.
- Read changes preserve payload, creation time, and resolved/unresolved state;
  active banner reminders remain active after marking all messages read.

## Asset-page banner polling

- Layer: component; local SQLite, injected balances and monotonic clock, no network.
- Entrypoint: `wallet_api::service::api_wallet::business_notification::tests::banners_check_all_balances_and_keep_review_and_read_states_separate`.
- One query returns UID-scoped pending review orders (existing `Init` predicate)
  and unresolved balance reminders as distinct typed groups with count/time/items.
- Orders with an audit decision are excluded even before their status advances.
- Each fresh query checks all unresolved reminders in the notification table,
  with four concurrent calls and a three-second request timeout. The frontend
  owns the polling timer. SDK buffers repeated calls for five seconds from check
  completion, per UID, and reads the latest local banner data during that window.
  A mutex serializes overlapping checks so waiting callers reuse the completed
  check. No per-call total limit or pagination cursor is used for balance IO.
- The final count and items are read after all balance checks finish. The test
  checks twelve records in each poll, then confirms five recoveries produce a
  count of seven in the same response.
- Failed balance checks preserve active reminders; marking read does not dismiss
  them. Threshold equality resolves a checked reminder.
- New reminders record `notifiedAt` in their payload from MQTT receipt, persisted
  through task replay. Group time uses it, falling back to `createdAt` for old rows;
  read and resolution updates do not change this reminder time.
- Resolution requires the checked payload to still match the stored row. The
  SQLite `stale_balance_observation_cannot_resolve_new_alert_payload` test covers
  stale thresholds, cross-UID attempts, and a matching observation.
- `banner_buffer_coalesces_calls_but_expires_and_invalidates_changed_alerts`
  covers overlapping calls, the five-second expiry boundary, UID isolation,
  read-state changes, new rows, and changed thresholds. New or changed payloads
  bypass the buffer; removals/read changes do not require another chain check.
- `banner_query_future_supports_plugin_thread_pool` checks the public banner
  future implements Send, as required by the generated FRB async wrapper.
- `business_all_read_covers_all_pages_without_resolving_or_crossing_uid`
  covers twelve reminders across pages (one resolved history item), idempotent all-read, unchanged active
  state and payloads, cross-UID isolation and invalid UID no-op on stored state.
  Unread count includes history; reading never resolves a banner.

## No-UID public withdrawal-wallet message center (2026-10-06)

These current public contracts supersede UID-only message-center statements above;
internal per-UID repo/service paths remain for ingestion and background work.

| Action | Required invariant | Component test |
| --- | --- | --- |
| Global list/count/read | Active Withdrawal scope, duplicate wallet rows do not duplicate reminders, global ordering precedes pagination; resolved unread history counts | `withdrawal_message_center_tests::global_message_center_filters_orders_pages_and_read_writes_by_active_wallet` |
| ID read and all-read | Recheck eligibility in SQL, exclude subaccounts/disabled/orphan/blank owner; repeated reads preserve payload and resolution | same SQLite test |
| Scope-table failure | Return errors, never mark out-of-scope or unread records read on failure | `missing_wallet_scope_table_fails_without_marking_any_message_read` |
| Public no-UID API | Preserve record ownership, no balance lock/chain IO on ordinary list/count/read; invalid page/ID rejected | `no_uid_message_api_keeps_owner_and_reads_without_balance_checks` |
| Existing unresolved refresh | Filter eligible scope before balance IO, resolve exact threshold with payload CAS, failed checks retain messages, repage after resolution | `global_unresolved_refresh_filters_before_chain_io_and_repages_after_resolution` |

No scale benchmark, App integration or live chain verification. Count/page are
separate existing SQL reads; concurrent mutations may change offset pagination.
Direct unresolved refresh retains serial balance IO and has no added total deadline;
background directed checks and pure local balance query remain separate.

## Lightweight presentation and local detail (2026-10-07)

| Boundary | Evidence |
| --- | --- |
| Summary exact id/kind/isRead/remaining/symbol/notifiedAt, no payload | projection_tests::summary_is_minimal_and_uses_notification_time |
| notifiedAt differs from first insert/update; legacy fallback only if absent | projection_tests timestamp tests |
| Typed detail coin/balance/merchant/address/chain/type; unknown/malformed rejected | projection_tests detail/error tests |
| DAO active withdrawal scope, duplicate wallets, excluded owners, SQL id binding, wallet disable | repositories::api_business_notification::detail_tests |
| Resolved history detail/list readable; banner unresolved; read does not mark read/resolve/RPC | service::api_wallet::business_notification::detail_tests |

No new withdrawal notification kind is persisted by this flow. Withdrawal order amount/tradeNo/detail/audit interfaces remain separate. Unknown kinds retain summary metadata with null balance fields, not an invented amount projection.

Kind typing: reuse BizType rather than introducing a second enum; mapping only admits currently persisted AWM_ADDRESS_INSUFFICIENT. Enum serialization/deserialization and unsupported event/numeric/case/blank strings are covered by projection_tests::existing_business_enum_preserves_wire_string_and_rejects_non_notification_kinds. Existing unknown-summary metadata and detail422 tests remain authoritative.
