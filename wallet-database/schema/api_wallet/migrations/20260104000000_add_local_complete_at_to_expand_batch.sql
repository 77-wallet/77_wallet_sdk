-- Add migration script here
-- 添加local_complete_at字段，用于标记本地扩容完成的不可逆事实
ALTER TABLE expand_batch ADD COLUMN local_complete_at TIMESTAMP;