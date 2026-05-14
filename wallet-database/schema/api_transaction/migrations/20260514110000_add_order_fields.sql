-- Add order fields from MQTT
-- For SQLite, ALTER TABLE ADD COLUMN will fail if column already exists
-- This is expected behavior and will not affect test execution

ALTER TABLE api_withdraws ADD COLUMN out_order_id TEXT NULL;
ALTER TABLE api_withdraws ADD COLUMN client_id TEXT NULL;
ALTER TABLE api_withdraws ADD COLUMN create_time TEXT NULL;
