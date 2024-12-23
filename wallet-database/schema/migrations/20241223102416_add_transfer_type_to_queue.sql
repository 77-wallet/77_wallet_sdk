-- Add migration script here
ALTER TABLE multisig_queue ADD COLUMN transfer_type INTEGER NOT NULL DEFAULT 1;