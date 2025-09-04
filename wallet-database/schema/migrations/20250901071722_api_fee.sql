-- Add migration script here
CREATE TABLE api_fee
(
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    uid              VARCHAR(20) NULL,                 -- 总钱包
    name             VARCHAR(64)             NOT NULL, -- 总钱包名称
    from_addr        VARCHAR(64)             NOT NULL,
    to_addr          VARCHAR(64)             NOT NULL,
    value            VARCHAR(64)             NOT NULL,
    chain_code       VARCHAR(64)             NOT NULL,
    token_addr       VARCHAR(128) NULL,
    symbol           VARCHAR(128) DEFAULT "" NOT NULL,
    trade_no         VARCHAR(32)             NOT NULL,
    trade_type       INTEGER                 NOT NULL,
    status           INTEGER                 NOT NULL,
    tx_hash          VARCHAR(32)             NOT NULL,
    transaction_fee  VARCHAR(256)            NOT NULL, --手续费
    resource_consume VARCHAR(256) DEFAULT "0",         --资源消耗
    transaction_time TIMESTAMP NULL,                   --交易时间
    block_height     VARCHAR(32) NULL,                 --块高
    notes            TEXT NULL,                        --备注
    post_tx_count    INTEGER      DEFAULT 0  NOT NULL, -- 已发送交易次数
    post_confirm_tx_count    INTEGER      DEFAULT 0  NOT NULL, -- 已确认交易次数
    created_at       TIMESTAMP               NOT NULL,
    updated_at       TIMESTAMP
);

CREATE INDEX api_fee_from ON api_fee (uid, from_addr);
CREATE UNIQUE INDEX api_fee_trade_no ON api_fee (trade_no);