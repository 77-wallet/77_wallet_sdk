ALTER TABLE api_withdraws ADD COLUMN broadcast_uncertain_since_at TIMESTAMP NULL;
ALTER TABLE api_withdraws ADD COLUMN broadcast_uncertain_retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE api_withdraws ADD COLUMN broadcast_uncertain_last_checked_at TIMESTAMP NULL;
ALTER TABLE api_withdraws ADD COLUMN broadcast_uncertain_reconciled_at TIMESTAMP NULL;
ALTER TABLE api_withdraws ADD COLUMN broadcast_uncertain_rebroadcast_count INTEGER NOT NULL DEFAULT 0;
