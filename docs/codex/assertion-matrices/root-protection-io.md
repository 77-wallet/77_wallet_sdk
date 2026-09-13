# Modern root keystore async I/O boundary

Scope: `ModernIo::store_root_with_protection` and `load_root_with_protection`.
Production callers still use the synchronous methods; this batch does not enable native protection.

The implementation is now shared in public `file_ops::protected::{store,load}`.
Root methods only serialize root data/select root.keystore and delegate; they no longer
implement encryption or filesystem operations. The shared API accepts an existing path
and opaque bytes, and its decoder can return any data type. It contains no wallet-record schema.

| Shared I/O scenario | Invariant | Test |
| --- | --- | --- |
| root.keystore, key0.keystore and verify via identical API | Arbitrary binary payload roundtrips through existing password crypto and outer callback; exact requested filenames; no temp files remain | same_io_handles_root_derived_and_verify_as_opaque_bytes |
| Protection fails for each filename | Existing bytes unchanged for every record kind | failed_protection_preserves_each_existing_file |

Run all shared/root regressions: `cargo test -p wallet-tree --offline --lib file_ops`.
This refactor provides the common entry point; existing derived/verify service callers have
not yet been switched from KeystoreBuilder and must not be described as protected.

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

## Shared migration replacement

`protected::rewrite` reads existing bytes and asks the caller for a validated optional
replacement. Some is published using the same atomic writer as store; None performs no
write. Password/version decisions and durable migration state remain outside wallet-tree.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Replacement followed by None | First replacement persisted, second call does not rewrite; no extra files | rewrite_commits_replacement_and_skips_current_bytes |
| Transform rejected | Original bytes and directory entries unchanged | rejected_rewrite_keeps_original_file |
| Two overlapping transforms | Second reads the first committed version, including case-alias filename on case-insensitive FS | concurrent_rewrites_observe_the_previous_committed_version |

Shared store/rewrite operations serialize by canonical parent directory in this process.
The publication worker owns the lock guard, so cancelling its waiter does not unlock a
still-running write. Callbacks must not await another store/rewrite in that directory.
Legacy writers outside the common API and other processes are not protected by this lock;
service migration must exclude those writers. This is not cross-process locking or a
durable migration-state implementation. Same-directory writes are conservatively serialized
to cover case-insensitive aliases; independent directories can proceed concurrently.
