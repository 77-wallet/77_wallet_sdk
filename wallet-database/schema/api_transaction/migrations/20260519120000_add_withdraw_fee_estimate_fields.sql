-- Add pre-execution fee/resource estimates for API withdraw audit pages.

ALTER TABLE api_withdraws ADD COLUMN estimated_transaction_fee TEXT NULL;
ALTER TABLE api_withdraws ADD COLUMN estimated_resource_consume TEXT NULL;
ALTER TABLE api_withdraws ADD COLUMN fee_estimated_at TIMESTAMP NULL;
