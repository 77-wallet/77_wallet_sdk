-- Add migration script here
ALTER TABLE task_queue
ADD COLUMN retry_times INTEGER NOT NULL DEFAULT 0;