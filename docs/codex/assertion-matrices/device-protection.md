# Device protection — SDK contract batch

Scope: platform-neutral provider contract. Production wallet routing is not connected yet.

| Scenario | Expected invariant | Test |
| --- | --- | --- |
| No provider configured | Both operations return NotConfigured; no pass-through | missing_provider_fails_both_operations |
| Injected provider | Operation, exact input and contextual AAD forwarded; provider result returned | forwards_bound_context_and_returns_only_provider_output |
| Native failure | Typed error preserved; input is not returned as a fallback | errors_propagate_without_returning_input |
| Context binding | Different scope/purpose pairs have unambiguous length-prefixed AAD; empty fields rejected | context_encoding_is_unambiguous_and_rejects_empty_fields |
| Deferred native result | Pending yields to executor, later poll delivers native result | pending_native_callback_yields_before_returning_result |

These fixture tests validate dispatch semantics, not encryption strength. Native adapters must enforce AAD, hardware security level, nonce generation and key lifecycle. No production storage protection claim until read/write paths are connected.

Validation of contract and storage tests:

```sh
cargo test -p wallet-api --offline --lib device_protection::
```

Production export compilation: `cargo check -p wallet-api --offline`.

## Existing-keystore byte compatibility

| Scenario | Expected invariant | Test |
| --- | --- | --- |
| New protected bytes read under either policy | No migration/re-wrapping; password decrypt works | new_file_never_enters_migration_or_double_wraps |
| Old bytes, wrong password | No hardware wrap or replacement returned | old_file_upgrades_once_only_after_correct_password |
| Old bytes, correct password, then read upgraded bytes | One upgrade, subsequent read returns no replacement | old_file_upgrades_once_only_after_correct_password |
| Device-required wallet sees legacy bytes | Reject downgrade before legacy decrypt | required_policy_and_unknown_formats_never_fall_back |
| Unknown/truncated version or native failure | Error, never legacy fallback | required_policy_and_unknown_formats_never_fall_back |
| Hardware unavailable during migration or wrong context on current data | No opened result/replacement; no rewrap | native_failure_does_not_release_legacy_migration_or_downgrade_current_data |

Tests use real existing wallet_crypto password encryption/decryption and a deterministic hardware-provider fixture. The seal/open methods transform bytes only. Callers must supply durable policy, atomically persist and read back replacement at the existing path, then update migration state. Those call sites are not wired yet; no App rollout/migration completion claim. The earlier unused file-store component was removed per user direction.

| KDF compatibility | Invariant | Test |
| --- | --- | --- |
| Excessive legacy work parameters | Reject before password/native work | excessive_kdf_cost_is_rejected_without_native_calls |
| Current Argon2id, Scrypt, PBKDF2 generation defaults | Accepted by resource policy | current_generated_kdf_profiles_remain_supported |

Resource limits: Argon2 memory <=256 MiB, iterations <=10, parallelism <=16 and memory-KiB × iterations <=1,048,576; Scrypt estimated memory <=256 MiB and N×r×p <=16,777,216; PBKDF2 iterations <=2,000,000. Derived length must be 32 bytes. Files outside these limits are left untouched and require an explicit compatibility/recovery decision; arbitrary historical/imported profiles have not been verified.

## SDK initialization binding

| Scenario | Expected invariant | Test |
| --- | --- | --- |
| Reuse cloned provider | Same Arc identity accepted | provider_identity_distinguishes_clone_replacement_and_disabled_mode |
| Replace provider or enable/disable after initialization | Identity mismatch; Context initializer rejects before WalletManager background startup | provider_identity_distinguishes_clone_replacement_and_disabled_mode |

The unit test covers the identity predicate; constructor wiring is checked by compilation/review, not a live backend/global-context test. WalletManager::new preserves legacy initialization; new_with_device_protection binds the injected provider before Context publication. No runtime setter. The plugin must retain/reuse the same provider object across retries. Wallet keystore I/O still needs to consume Context.device_protection; constructor injection alone does not enable storage protection.

## SDK modern-root codec binding

`KeystoreProtection::store_root` and `load_root` now bind the tree's existing-path async
root operations to the injected provider. AAD uses stable wallet identity as scope and
`root-keystore` as purpose. These methods are not yet called by wallet services.

| Scenario | Expected invariant | Test |
| --- | --- | --- |
| Store, recreate codec, read | WDK v1 persisted at root.keystore; correct root recovered; no rewrap or extra files | root_file_roundtrip_uses_device_codec_and_survives_reopening |
| Wrong password / wrong wallet / missing provider | Respective password/authentication/configuration errors retained; stored bytes unchanged | root_read_errors_preserve_file_and_error_category |
| Write without provider | NotConfigured; old file retained; no extra files | root_missing_provider_write_does_not_replace_existing_file |
| Legacy root supplied to protected read | LegacyForbidden before native operations; file unchanged | root_read_rejects_legacy_without_migration_or_rewrite |

Command: `cargo test -p wallet-api --offline --lib device_protection`.
Real password crypto and temporary files, deterministic provider fixture only; no hardware assurance
from these tests. Tree I/O failures are sanitized as Storage; native/password errors stay distinct.
Protection/decoder failure invariants do not promise rollback after an atomic replacement's directory
sync fails. Competing writes and blocking-worker cancellation require caller lifecycle coordination.
Safe first-use provisioning, service cutover, other file/SQLite paths and durable migration remain pending.

## Common SDK file binding

`KeystoreProtection::store_file/load_file` are the common SDK byte interfaces for all
keystore files. They use shared wallet-tree I/O and seal/open; root methods are only
serialization/filename adapters. They require the caller's expected ProtectionContext,
not an identity supplied by the file. Loaded plaintext uses Zeroizing<Vec<u8>>.

| Scenario | Invariant | Test |
| --- | --- | --- |
| Same SDK API with root, derived and verify paths | WDK format, binary payload recovery, repeated reads do not rewrap, no extra files | common_file_codec_roundtrips_every_keystore_kind_without_rewrap |
| Root ciphertext copied to derived path | Expected derived context rejects it; file unchanged; no rewrap | common_file_codec_rejects_cross_record_substitution |

The tests use synthetic record-purpose labels and a deterministic provider; production
callers still need stable record-identity conventions and activation/migration orchestration.
This binding alone does not switch existing service I/O or protect SQLite fields.
