# Modern root keystore async I/O boundary

Scope: `ModernIo::store_root_with_protection` and `load_root_with_protection`.
Production callers still use the synchronous methods; this batch does not enable native protection.

| Scenario | Result / persisted invariant | Test |
| --- | --- | --- |
| Asynchronous protect and decode | Existing root.keystore filename; only transformed bytes on disk; existing password decryptor recovers RootData; no temp files | root_roundtrip_uses_existing_password_format_and_path |
| Protection callback fails | Error propagated; original ciphertext unchanged; no extra files | failed_protection_keeps_existing_root_and_creates_no_files |
| Decoder fails | Error propagated; file unchanged; no rewrite or legacy fallback | failed_decoder_never_changes_existing_root |
| File exceeds size bound | Decoder not called; file unchanged | oversized_file_is_rejected_before_decoder |
| Protection returns empty bytes | Rejected before disk write; original file unchanged | empty_protection_result_is_rejected_without_truncating_root |
| Atomic publication fails | Existing destination untouched; temporary ciphertext cleaned up | failed_publication_cleans_temporary_ciphertext |
| Protection suspends, then succeeds | Old file untouched while awaiting; then replaced; no temporary files left | replacement_waits_for_protection_before_touching_old_root |

Validation: `cargo test -p wallet-tree --offline --lib file_ops::v2`.
Fixtures contain synthetic bytes only, use real password encryption, and write under target.
The test marker is not encryption and provides no hardware assurance.

Caller obligations: bind a trusted device codec to a stable wallet context, provision only after
checking protected-record state, serialize competing writes, and enforce durable migration policy.
The load callback must validate format/password and return RootData; reads never migrate automatically.
Do not activate the new writer before all other root consumers support its format.
Derived/verify files, password changes, SQLite secrets and wallet App integration are outside this batch.

Atomic rename protects the old file before publication. A parent-directory sync failure after rename
reports an uncertain durability outcome; cancellation cannot stop an already running blocking write.
This is not a multi-file transaction or crash-recovery implementation.
