-- Add collect-side fact for backend fee order receipt
ALTER TABLE api_collect
ADD COLUMN service_fee_order_received_at TIMESTAMP NULL;
