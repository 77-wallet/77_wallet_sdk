-- Add migration script here
CREATE INDEX IF NOT EXISTS idx_api_account_chain_status ON api_account (chain_code, status);