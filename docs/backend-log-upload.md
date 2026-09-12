# Backend log upload configuration and migration

Transport support is in `wallet_transport_backend::log_upload`.
The client sends multipart `key`, `secret`, `sn`, and `file` to the configured complete URL.
The `sn` is supplied at upload time, not stored as a shared configuration value.
The file name is multipart metadata; the server chooses the final OSS object key.

`LogUploadConfig` fields: `url`, `key`, `secret` (all required strings).
Credentials are redacted from Debug and are not included in errors.
A response succeeds only if HTTP is 2xx and JSON contains boolean `success: true`,
consistent with existing backend responses. `code`, `msg`, and `data` are not needed
for the upload acknowledgement. String data does not require AES decryption because
its value is not consumed. Empty bodies and missing/non-boolean success are errors.
No automatic retry or redirect is performed. The acknowledgement limit is 64 KiB;
connect timeout 10 seconds, whole request timeout 60 seconds.

## Migration status

Batch 2 wires automatic uploads to the backend. `wallet-api::Config` accepts optional
`log_upload`. Missing configuration disables automatic uploads (no OSS fallback).
Invalid configured credentials or URL fail initialization. Manual uploads still use
OSS, so the existing `oss` section remains required until the next batch; add the new
section alongside it rather than deleting it. The plugin must use this SDK revision
before this takes effect. Existing AES response settings are unchanged.

Automatic uploads check every 5 minutes (Tokio interval also ticks immediately on startup).
Pending content below 1 KiB is retained. The worker reads the current log through one
file handle and commits its identity/offset only after backend acknowledgement.
Upload failure leaves the in-memory and persisted checkpoint unchanged for retry.
This batch does not add backfill across all rotated log archives. Checkpoint persistence
still uses the existing OffsetTracker best-effort save implementation.

Add this section to the existing YAML after upgrading the SDK dependency:

```yaml
log_upload:
  url: "https://logs.example.invalid/log/uploadFile"
  key: "REPLACE_LOCALLY"
  secret: "REPLACE_LOCALLY"
```
