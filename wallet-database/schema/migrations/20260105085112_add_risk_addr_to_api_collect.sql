-- Add migration script here
ALTER TABLE api_collect
ADD COLUMN risk_addr INTEGER DEFAULT 0 NOT NULL,