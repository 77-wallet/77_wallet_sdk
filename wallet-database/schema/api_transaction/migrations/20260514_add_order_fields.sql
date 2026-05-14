-- Add order fields from MQTT
ALTER TABLE api_withdraws
ADD COLUMN out_order_id TEXT NULL;

ALTER TABLE api_withdraws
ADD COLUMN client_id TEXT NULL;

ALTER TABLE api_withdraws
ADD COLUMN create_time TEXT NULL;
