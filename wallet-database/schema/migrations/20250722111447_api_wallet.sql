-- Add migration script here
CREATE TABLE api_wallet (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 增加唯一主键
    name VARCHAR(64) NOT NULL,
    uid VARCHAR(64) NOT NULL,
    address VARCHAR(64) NOT NULL UNIQUE,
    -- address 建唯一约束即可，无需再作为主键
    phrase TEXT NOT NULL,
    -- 助记词可能会超过 64 字符
    seed TEXT,
    -- 同上，长度可能更长
    wallet_type INTEGER NOT NULL,
    merchant_id TEXT,
    app_id TEXT,
    status INTEGER NOT NULL,
    is_init INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP
);
CREATE INDEX api_wallet_uid_idx ON api_wallet (uid);
CREATE INDEX api_wallet_merchant_id_idx ON api_wallet (merchant_id);
CREATE INDEX api_wallet_app_id_idx ON api_wallet (app_id);