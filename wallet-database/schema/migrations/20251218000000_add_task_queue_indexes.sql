-- Add migration script here
-- 添加task_queue表的关键索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_task_queue_name_status ON task_queue(task_name, status);
CREATE INDEX IF NOT EXISTS idx_task_queue_type_status ON task_queue(type, status);
CREATE INDEX IF NOT EXISTS idx_task_queue_created_at ON task_queue(created_at);
CREATE INDEX IF NOT EXISTS idx_task_queue_status ON task_queue(status);
