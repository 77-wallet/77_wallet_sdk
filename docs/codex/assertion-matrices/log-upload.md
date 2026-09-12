# Backend log upload — batch 1 transport

Scope: wallet-transport-backend only. Existing automatic/manual OSS upload is not
switched in this batch. Backend contract confirmed by user: same envelope as existing
interfaces; HTTP 2xx and success=true required. Response data is unused.

| Invariant | Offline test |
| --- | --- |
| POST multipart has key, secret, runtime sn and exact file bytes/name | multipart_contains_auth_device_and_exact_file_content |
| Invalid endpoint/auth/file metadata fails before network IO | invalid_config_and_metadata_are_rejected_before_network_io |
| Debug does not expose auth | config_debug_does_not_reveal_credentials |
| HTTP failure, redirects, business rejection, empty/malformed JSON cannot acknowledge | acknowledgement_requires_http_and_business_success |
| Actual upload accepts success, refuses 307/308, rejects oversized chunked ack | upload_executes_against_fake_backend_and_refuses_redirects_and_oversized_ack |

Client disables redirects, has 10s connection/60s request timeouts, bounds acknowledgement
body to 64 KiB, and emits no request or response content in errors. HTTPS should be used
for deployment; HTTP remains accepted for the endpoint supplied by the backend.

Run: `cargo test --offline -p wallet-transport-backend --no-default-features --lib log_upload`.
The transport test binds an ephemeral 127.0.0.1 port and may require sandbox permission.
Never run these checks with production credentials or a remote endpoint.

## Batch 2 — wallet-api automatic uploads

| Invariant | Test |
| --- | --- |
| Incomplete upload config is rejected rather than ignored | log_upload_config_rejects_incomplete_credentials |
| New section parses; old config remains valid with uploads disabled | log_upload_config_accepts_new_section_and_preserves_legacy_parsing |
| Failure preserves offset, identity and on-disk checkpoint; retry sends identical pending bytes and runtime SN | failed_upload_preserves_checkpoint_and_retry_sends_same_pending_bytes |
| Rotation changes checkpoint only after acknowledgement | rotated_file_commits_new_identity_only_after_acknowledgement |
| Under 1 KiB causes neither upload nor checkpoint change | less_than_one_kib_waits_without_upload_or_checkpoint_change |
| No log_upload config means no background task or OSS fallback | missing_config_starts_no_background_upload_task |

Run: `cargo test --offline -p wallet-api --no-default-features --lib log_upload_config`
and `cargo test --offline -p wallet-api --no-default-features --lib infrastructure::log::upload_log`.
Worker tests use an in-memory fake uploader and temporary synthetic files; no sockets.
