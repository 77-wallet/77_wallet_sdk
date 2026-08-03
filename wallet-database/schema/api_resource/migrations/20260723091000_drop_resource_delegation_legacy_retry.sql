-- Resource delegation now advances from broadcast and completion facts only.

DROP INDEX IF EXISTS api_resource_delegation_scan;

ALTER TABLE api_resource_delegation
    DROP COLUMN recover_status;

ALTER TABLE api_resource_delegation
    DROP COLUMN next_retry_at;

CREATE INDEX api_resource_delegation_scan
    ON api_resource_delegation (source, operation_type, status, created_at);
