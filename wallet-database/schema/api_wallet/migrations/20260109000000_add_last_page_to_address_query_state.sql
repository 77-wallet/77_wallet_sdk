-- Add migration script here
-- 为地址查询状态表添加 last_page 字段
ALTER TABLE address_query_state
ADD COLUMN last_page INTEGER NOT NULL DEFAULT 0;