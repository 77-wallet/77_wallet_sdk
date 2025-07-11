-- Add migration script here
ALTER TABLE task_queue ADD COLUMN wallet_type VARCHAR DEFAULT "normal" NULL;