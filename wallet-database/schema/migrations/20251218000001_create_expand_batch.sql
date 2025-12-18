-- Add migration script here
-- 创建扩容批次表，用于跟踪扩容进度
CREATE TABLE IF NOT EXISTS expand_batch (
    batch_id TEXT PRIMARY KEY,
    chain_code TEXT NOT NULL,
    total_count INTEGER NOT NULL,
    finished_count INTEGER NOT NULL DEFAULT 0 CHECK (finished_count <= total_count),
    status INTEGER NOT NULL DEFAULT 0, -- 0=running, 1=done
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER
);

-- 添加索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_expand_batch_chain_status ON expand_batch(chain_code, status);
CREATE INDEX IF NOT EXISTS idx_expand_batch_created_at ON expand_batch(created_at);
