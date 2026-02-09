-- Add migration script here
ALTER TABLE api_account
ADD COLUMN uid VARCHAR(64) NOT NULL DEFAULT "";
CREATE INDEX api_account_uid_chaincode_idx ON api_account(uid, chain_code);