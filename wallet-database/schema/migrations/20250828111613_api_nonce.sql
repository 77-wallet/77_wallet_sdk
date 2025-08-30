-- Add migration script here

CREATE TABLE api_nonce
(
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    uid              VARCHAR(20) NULL,                 -- 总钱包
    name             VARCHAR(64)             NOT NULL, -- 总钱包名称
    from_addr        VARCHAR(64)             NOT NULL,
    chain_code       VARCHAR(64)             NOT NULL,
    nonce  INTEGER,
    created_at       TIMESTAMP               NOT NULL,
    updated_at       TIMESTAMP
);

CREATE UNIQUE INDEX api_nonce_from ON api_nonce (uid, from_addr, chain_code);
