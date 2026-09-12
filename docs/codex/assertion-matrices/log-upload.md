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
