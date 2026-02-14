-- Add migration script here
--
-- tx_res_received_at semantics:
-- - "SDK has received and persisted AWM_ORDER_TRANS_RES (SER tx result push)"
-- - This is NOT equivalent to on-chain confirmation (transaction_time).
-- - It is a hard ordering gate for sending TransAckType::TX_RES back to backend.

ALTER TABLE api_withdraws ADD COLUMN tx_res_received_at TIMESTAMP NULL;
ALTER TABLE api_collect ADD COLUMN tx_res_received_at TIMESTAMP NULL;
ALTER TABLE api_fee ADD COLUMN tx_res_received_at TIMESTAMP NULL;

