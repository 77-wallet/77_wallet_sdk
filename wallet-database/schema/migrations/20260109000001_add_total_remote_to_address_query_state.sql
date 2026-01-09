-- Add migration script here
-- 向address_query_state表添加total_remote字段
ALTER TABLE address_query_state
ADD COLUMN total_remote INTEGER NOT NULL DEFAULT 0;