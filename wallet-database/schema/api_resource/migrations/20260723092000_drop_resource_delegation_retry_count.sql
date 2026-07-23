-- Resource delegation retries are represented by broadcast facts, not a generic counter.

ALTER TABLE api_resource_delegation
    DROP COLUMN retry_count;
