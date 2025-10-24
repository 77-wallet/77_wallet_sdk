-- Add migration script here
ALTER TABLE task_queue ADD COLUMN err_msg TEXT DEFAULT NULL;