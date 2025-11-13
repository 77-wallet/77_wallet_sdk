-- Add migration script here
ALTER TABLE task_queue ADD COLUMN remark TEXT DEFAULT NULL;
