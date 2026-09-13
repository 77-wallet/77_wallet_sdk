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

## Read-only existing-file inventory

`file_ops::inventory::scan_wallet` discovers modern and legacy root/subs records;
`scan_verify` discovers the separate App-root verify file. These functions never read
secret contents or change files, migration progress or native keys. Empty results are
not permission to provision a key. SQLite secrets and authoritative DB membership
must be checked by the caller before rollout. Unknown files (including orphan temporary
files) currently block scanning and require recovery; they are never silently discarded.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Mixed modern/legacy files and verify | All six wallet identities found; only public derived_meta excluded; contents unchanged | scans_all_existing_modern_and_legacy_keystores_without_reading_contents |
| Unknown file or different wallet root | Error, original bytes preserved | unknown_file_and_wrong_wallet_root_are_not_silently_skipped |
| Empty existing directory | Empty result; no files or progress created | empty_directories_do_not_create_data_or_progress |
| Symbolic link | Error without following/replacing link target | symlink_is_rejected_without_following_or_replacing_it |
| Different names decoding to one identity | Error, both files preserved | duplicate_decoded_identity_is_rejected_without_changing_files |

Validation: `cargo test -p wallet-tree --offline --lib file_ops`.

## Standard-wallet phrase service integration

Scope: actual `WalletService::get_phrase`, modern WDK root read with injected provider.
The service delegates root loading to `WalletApplication::load_root`; migration policy
and reader selection live there, while `keystore_format::has_device_header` owns the
format probe. This extraction preserves existing behavior and uses the same service tests.
This read path does not enable writes, provision keys or migrate legacy data. Other
business consumers still require integration before wallet-wide activation.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Protected root, new Context on same directory, wrong password and native Locked | Actual service returns phrase before/after reopen; typed password/Locked errors propagated; file unchanged | protected_phrase_uses_actual_service_after_context_reopen |
| Legacy root then completed modern migration with missing modern file | Legacy initially readable without native calls; completion forbids fallback to retained old files | legacy_phrase_stays_readable_but_completed_migration_blocks_rollback |

| Modern password-only root, unsupported WDK version | Existing modern reader works with zero native calls; unsupported WDK fails without fallback or rewriting | modern_password_only_phrase_uses_existing_reader_without_native_calls |

Validation: `cargo test -p wallet-api --offline --lib protection_tests`.
These are offline service component tests with real SQLite/password crypto and synthetic
native envelopes, not hardware or wallet App runtime tests. No global Context is initialized.

## Root seed consumers and password fallback

`get_seed` delegates to the same root loader used by phrase export. Derivation import,
account/chain seed pass-throughs and derived-address listing use that entry. The standard
wallet branch of `try_decrypt_wallet_db` also uses the root loader. Only password failures
continue the existing fallback; typed device failures and policy/state failures propagate.
This does not activate protected writes or change the existing password_proof fast path.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Protected seed and password fallback, then wrong password and Locked | Exact seed recovered; valid password accepted; wrong password false; Locked propagated and root bytes unchanged | protected_seed_and_password_fallback_use_device_reader |
| Legacy password fallback | Correct password true, wrong password false; zero native calls | legacy_password_fallback_preserves_success_and_wrong_password_behavior |

Validation: `cargo test -p wallet-api --offline --lib protection`.

## Root password change through AccountDomain

`AccountDomain::set_root_password` keeps the wallet existence check and delegates to
`WalletApplication::change_root_password`. Existing-root read/write selection shares
`record_requires_device`. WDK records use the generic codec `change_file_password` under
the common directory rewrite lock; the existing password generator transforms opaque
plaintext in memory, and only validated outer ciphertext reaches the atomic publisher.
Legacy records retain their previous writer. No keys are provisioned and no migration
state is changed by this operation.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Native protect failure, incorrect old password, then successful change | Failure preserves original bytes and old-password access; success retains WDK and identical phrase/seed; old password fails; no extra files | root_password_change_preserves_wdk_and_recovers_from_native_failure |
| Two simultaneous changes with same old password | Exactly one succeeds; winning password reads original phrase; no extra files | root_password_change_preserves_wdk_and_recovers_from_native_failure |
| Completed root removed with old root files retained | Password change fails without modifying old files | completed_root_password_change_rejects_legacy_rollback |
| Legacy root change | New password reads phrase, old password fails; existing names retained and zero native unwrap calls | legacy_root_password_change_keeps_existing_format |

Validation: `cargo test -p wallet-api --offline --lib protection`.
The atomicity guarantee is per protected file, before publication. The existing App-wide
password change updates multiple records sequentially and is not made transactional by
this batch. Failures after atomic rename can still report uncertain durability, as above.

## Derived account-file password change

The actual `AccountDomain::set_account_password` entry delegates to the same generic
protected password transform as root changes. Expected context is wallet address plus
`derived:{account_id}`, at the existing `subs/key{account_id}.keystore` path. Read-policy
selection is shared as `record_requires_device`. Protected payloads are not parsed or
rebuilt, and this path never writes `derived_meta.json` or deletes the original first.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Multi-key protected file: wrong password/native failure, then successful change | Exact plaintext retained; WDK retained; index and other account file byte-identical; failures preserve original; old password fails after success | derived_password_change_preserves_payload_index_and_other_accounts |
| Completed account restored to password-only file | LegacyForbidden; ciphertext and index unchanged | completed_derived_password_change_rejects_legacy_rollback |
| Existing password-only V2 file | New password recovers same payload; old password fails; no native calls | password_only_derived_change_remains_compatible |

Validation: `cargo test -p wallet-api --offline --lib protection`.
Fixtures use the existing ModernIo writer to produce actual grouped account payloads.
This covers account-file password changes, not single-address `set_sub_password`,
private-key read activation, multi-file transactionality, or the pre-existing V1
`load_account` unimplemented branch. No new protected-file activation is introduced.
