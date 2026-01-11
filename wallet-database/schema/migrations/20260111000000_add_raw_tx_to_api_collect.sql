-- Add migration script here
ALTER TABLE api_collect ADD COLUMN raw_tx TEXT NULL;
ALTER TABLE api_withdraws ADD COLUMN raw_tx TEXT NULL;
ALTER TABLE api_fee ADD COLUMN raw_tx TEXT NULL;