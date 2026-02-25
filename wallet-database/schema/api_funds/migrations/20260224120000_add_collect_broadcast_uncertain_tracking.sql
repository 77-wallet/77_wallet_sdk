-- EVM uncertain lifecycle tracking for collect (broadcast hash returned but same-RPC not visible)
-- Used to bound retries and auto-close uncertain orders without manual intervention.

ALTER TABLE api_collect ADD COLUMN broadcast_uncertain_since_at TIMESTAMP NULL;
ALTER TABLE api_collect ADD COLUMN broadcast_uncertain_retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE api_collect ADD COLUMN broadcast_uncertain_last_checked_at TIMESTAMP NULL;
ALTER TABLE api_collect ADD COLUMN broadcast_uncertain_reconciled_at TIMESTAMP NULL;
ALTER TABLE api_collect ADD COLUMN broadcast_uncertain_rebroadcast_count INTEGER NOT NULL DEFAULT 0;
