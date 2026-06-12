-- Speed up API wallet foreground operation detail lookup by chain hash.

CREATE INDEX IF NOT EXISTS api_resource_operation_hash_owner
    ON api_resource_operation (tx_hash, owner_address);
