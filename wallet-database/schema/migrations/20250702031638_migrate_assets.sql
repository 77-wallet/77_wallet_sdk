-- Add migration script here
-- 1. 创建新表（去除原主键，新增 wallet_type）
CREATE TABLE IF NOT EXISTS assets_v2 (
    name TEXT NOT NULL,
    symbol TEXT NOT NULL,
    decimals INTEGER NOT NULL,
    address TEXT NOT NULL,
    token_address TEXT,
    protocol TEXT,
    chain_code TEXT NOT NULL,
    status INTEGER NOT NULL,
    is_multisig INTEGER NOT NULL DEFAULT 0,
    balance TEXT,
    wallet_type TEXT NOT NULL DEFAULT 'normal',
    created_at TEXT NOT NULL,
    updated_at TEXT
);

-- 2. 拷贝原始数据（wallet_type 默认设为 normal）
INSERT INTO assets_v2 (
    name, symbol, decimals, address, token_address, protocol,
    chain_code, status, is_multisig, balance,
    wallet_type, created_at, updated_at
)
SELECT
    name, symbol, decimals, address, token_address, protocol,
    chain_code, status, is_multisig, balance,
    'normal' AS wallet_type, created_at, updated_at
FROM assets;

-- 3. 删除旧表
DROP TABLE assets;

-- 4. 重命名新表为原表名
ALTER TABLE assets_v2 RENAME TO assets;

-- 5. 创建唯一索引（代替旧主键）
CREATE UNIQUE INDEX IF NOT EXISTS uniq_assets_composite
ON assets(symbol, address, chain_code, token_address, wallet_type);

-- 6. 可选查询优化索引
CREATE INDEX IF NOT EXISTS idx_assets_wallet
ON assets(address, chain_code, wallet_type);
