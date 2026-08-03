-- Chain outcome is represented by chain_success_at and chain_failed_at facts.

ALTER TABLE api_resource_delegation
    DROP COLUMN tx_status;
