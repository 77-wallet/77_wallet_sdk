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

Batch 1 adds transport support only. `wallet-api::Config` still expects `oss`;
DO NOT replace plugin config.yaml with log_upload yet. The next batch must wire the
new config/client to automatic uploads and preserve offsets on any failure. Manual
uploads and removal of obsolete OSS configuration follow in a separate batch.
Existing AES response settings are outside this migration.

The proposed YAML after wallet-api migration (not usable with current wallet-api):

```yaml
log_upload:
  url: "https://logs.example.invalid/log/uploadFile"
  key: "REPLACE_LOCALLY"
  secret: "REPLACE_LOCALLY"
```
