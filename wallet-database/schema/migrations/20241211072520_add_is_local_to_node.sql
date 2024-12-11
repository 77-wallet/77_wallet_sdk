-- Add migration script here
ALTER TABLE node ADD COLUMN is_local INTEGER NOT NULL DEFAULT 0;