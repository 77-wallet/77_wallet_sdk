-- Add migration script here
CREATE TABLE api_withdraws (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid VARCHAR(20) NULL,
    from_addr VARCHAR(64) NOT NULL,
    to_addr VARCHAR(64) NOT NULL,
    value VARCHAR(64) NOT NULL,
    decimals INTEGER NOT NULL,
    token_address VARCHAR(128) DEFAULT "" NOT NULL,
    token_code VARCHAR(128) DEFAULT "" NOT NULL,
    trade_no VARCHAR(32) NOT NULL,
    trade_type VARCHAR(32) NOT NULL,
    status INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP
);
CREATE INDEX api_withdraws_from ON api_withdraws (uid, from_addr);
CREATE UNIQUE INDEX api_withdraws_trade_no ON api_withdraws (trade_no);