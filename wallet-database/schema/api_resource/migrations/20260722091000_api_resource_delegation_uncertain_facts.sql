ALTER TABLE api_resource_delegation
ADD COLUMN broadcast_uncertain_since_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
ADD COLUMN broadcast_uncertain_retry_count INTEGER DEFAULT 0 NOT NULL;

ALTER TABLE api_resource_delegation
ADD COLUMN broadcast_uncertain_last_checked_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
ADD COLUMN broadcast_uncertain_reconciled_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
ADD COLUMN broadcast_uncertain_rebroadcast_count INTEGER DEFAULT 0 NOT NULL;
