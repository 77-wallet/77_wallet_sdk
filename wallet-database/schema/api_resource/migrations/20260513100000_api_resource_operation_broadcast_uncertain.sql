-- Add broadcast uncertain tracking fields for resource operation.
-- These fields are used to track broadcast failures and implement retry/timeout logic.

ALTER TABLE api_resource_operation
ADD COLUMN broadcast_uncertain_since_at TIMESTAMP NULL;

ALTER TABLE api_resource_operation
ADD COLUMN broadcast_uncertain_retry_count INTEGER NOT NULL DEFAULT 0;

ALTER TABLE api_resource_operation
ADD COLUMN broadcast_uncertain_last_checked_at TIMESTAMP NULL;

ALTER TABLE api_resource_operation
ADD COLUMN broadcast_uncertain_reconciled_at TIMESTAMP NULL;
