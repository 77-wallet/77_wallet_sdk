-- Add migration script here
CREATE TABLE api_withdraws
(
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    uid              VARCHAR(20) NULL,                 -- 总钱包
    name             VARCHAR(64)             NOT NULL, -- 总钱包名称
    from_addr        VARCHAR(64)             NOT NULL,
    to_addr          VARCHAR(64)             NOT NULL,
    value            VARCHAR(64)             NOT NULL,
    validate VARCHAR(64) NOT NULL,
    chain_code       VARCHAR(64)             NOT NULL,
    token_addr       VARCHAR(128) NULL,
    symbol           VARCHAR(128) DEFAULT "" NOT NULL,
    trade_no         VARCHAR(32)             NOT NULL,
    trade_type       INTEGER                 NOT NULL,
    init_status      INTEGER         DEFAULT 0        NOT NULL,
    status           INTEGER         DEFAULT 0        NOT NULL,
    tx_hash          VARCHAR(32)             NOT NULL,
    transaction_fee  VARCHAR(256)            NOT NULL, --手续费
    resource_consume VARCHAR(256) DEFAULT "0",         --资源消耗
    transaction_time TIMESTAMP NULL,                   --交易时间
    block_height     VARCHAR(32) NULL,                 --块高
    notes            TEXT NULL,                        --备注
    post_tx_count    INTEGER       DEFAULT 0 NOT NULL, -- 已发送交易次数
    post_confirm_tx_count    INTEGER      DEFAULT 0  NOT NULL, -- 已确认交易次数
    created_at       TIMESTAMP               NOT NULL,
    updated_at       TIMESTAMP
);

CREATE INDEX api_withdraws_from ON api_withdraws (uid, from_addr, trade_type, status);
CREATE INDEX api_withdraws_hash ON api_withdraws (tx_hash);
CREATE UNIQUE INDEX api_withdraws_trade_no ON api_withdraws (trade_no);