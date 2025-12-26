-- Add migration script here
ALTER TABLE api_chain ADD COLUMN node_bind_type INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chain ADD COLUMN node_bind_type INTEGER NOT NULL DEFAULT 0;
