-- Add migration script here
CREATE TABLE api_assets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name VARCHAR(64) NOT NULL,
    symbol VARCHAR(64) NOT NULL,
    decimals INTEGER NOT NULL,
    address VARCHAR(28) NOT NULL,
    token_address VARCHAR(128) DEFAULT "" NOT NULL,
    protocol VARCHAR(20) NULL,
    chain_code VARCHAR(32) NOT NULL,
    status INTEGER NOT NULL,
    is_multisig INTEGER DEFAULT 0 NOT NULL,
    balance VARCHAR(256) NULL,
    type INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP
);

CREATE UNIQUE INDEX api_assets_symbol_address_chain_token_idx ON api_assets (symbol, address, chain_code, token_address);
CREATE INDEX api_assets_symbol_address_chain_idx ON api_assets (symbol, address, chain_code);