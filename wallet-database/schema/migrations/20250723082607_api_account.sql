-- Add migration script here
CREATE TABLE api_account (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id INTEGER NOT NULL,
    name VARCHAR(64) NOT NULL,
    address VARCHAR(128) NOT NULL,
    pubkey VARCHAR(128),
    private_key VARCHAR(128),
    address_type VARCHAR(64) DEFAULT '',
    wallet_address VARCHAR(128) NOT NULL,
    derivation_path VARCHAR(32),
    derivation_path_index VARCHAR(32),
    chain_code VARCHAR(32) NOT NULL,
    wallet_type TEXT NOT NULL,
    status INTEGER NOT NULL,
    is_init INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP
);
-- 唯一索引：避免同 address + chain_code + address_type 三元组重复
CREATE UNIQUE INDEX api_account_address_chain_code_idx ON api_account (address, chain_code, address_type);
-- 加速通过 wallet_address 查找 account 的场景
CREATE INDEX api_account_wallet_address_idx ON api_account (wallet_address);