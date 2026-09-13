# Device-protection migration progress

Scope: wallet-database MigrationRepo; existing config table, internal types=1 rows.
No schema changes. State is operational progress, not tamper-proof authorization.
Absence does not mean legacy access is permitted or native keys may be created.

| Scenario | Persisted invariant | Test |
| --- | --- | --- |
| Begin, complete, close/reopen database | DeviceRequired persists | completed_state_survives_reopen_and_cannot_be_restarted |
| Retry/overlap begin and complete after completion | State never regresses to Pending | completed_state_survives_reopen_and_cannot_be_restarted |
| Complete without begin / empty scope | Error; no migration row created | completion_without_begin_does_not_create_state |
| Unknown stored version | get/begin/complete reject; existing value unchanged | unknown_state_is_preserved_and_never_reset |
| Completion races first begin | Completion returns DeviceRequired or NotStarted; never successful Pending | completion_racing_with_first_begin_never_reports_pending_success |

Run `cargo test -p wallet-database --offline --lib repositories::device_protection`.
Component tests use actual SQLite schema initialization and reopen under target; no backend.

Callers must use stable non-secret scope IDs and verify every record in a scope before
marking it complete. New/old-data detection, safe native provisioning, file migration,
and integrating this state into SDK service policy remain separate work. A plain SQLite
row can be externally tampered with or rolled back, so it cannot independently prove a
wallet is protected or authorize fallback to legacy files. No reset API is provided.
