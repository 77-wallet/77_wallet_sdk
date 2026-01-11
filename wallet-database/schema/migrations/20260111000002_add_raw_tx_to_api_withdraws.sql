-- Add migration script here
ALTER TABLE api_withdraws ADD COLUMN raw_tx TEXT NULL;