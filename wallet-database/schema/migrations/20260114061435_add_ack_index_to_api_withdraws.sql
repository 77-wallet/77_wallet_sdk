-- Add index for ACK fields in api_withdraws table
-- Created: 2026-01-14
ALTER TABLE api_withdraws
ADD COLUMN tx_ack_sent_at TIMESTAMP NULL;
ALTER TABLE api_withdraws
ADD COLUMN tx_res_ack_sent_at TIMESTAMP NULL;
CREATE INDEX api_withdraws_ack_times ON api_withdraws (tx_ack_sent_at, tx_res_ack_sent_at);