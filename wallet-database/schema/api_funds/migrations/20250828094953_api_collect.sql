-- Add migration script here
CREATE TABLE api_collect
(
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    uid                   VARCHAR(20) NULL,                 -- 总钱包
    name                  VARCHAR(64)             NOT NULL, -- 总钱包名称
    from_addr             VARCHAR(64)             NOT NULL,
    to_addr               VARCHAR(64)             NOT NULL,
    value                 VARCHAR(64)             NOT NULL,
    validate              VARCHAR(64)             NOT NULL,
    chain_code            VARCHAR(64)             NOT NULL,
    token_addr            VARCHAR(128) NULL,
    symbol                VARCHAR(128) DEFAULT "" NOT NULL,
    trade_no              VARCHAR(32)             NOT NULL,
    trade_type            INTEGER                 NOT NULL,
    risk_addr             INTEGER      DEFAULT 0  NOT NULL, -- 0 默认值，无意义 1 正常地址 2 风险地址
    status                INTEGER                 NOT NULL, -- UI/人类可读状态，不参与执行逻辑
    nonce                 INTEGER      DEFAULT 0  NOT NULL, -- nonce
    risk_addr             INTEGER      DEFAULT 0  NOT NULL, -- 是否风险地址
    tx_hash               VARCHAR(32)             NOT NULL, -- hash
<<<<<<< HEAD:wallet-database/schema/migrations/20250828094953_api_collect.sql
    raw_tx                TEXT NULL,                        -- 交易原始数据
=======
    raw_tx                TEXT NULL,                        -- 原始交易
    transaction_fee       VARCHAR(256)            NOT NULL, -- 手续费
>>>>>>> ca4b1c2f2dd8305abbb165a1a59cfba227194dda:wallet-database/schema/api_funds/migrations/20250828094953_api_collect.sql
    resource_consume      VARCHAR(256) DEFAULT "0",         -- 资源消耗
    transaction_fee       VARCHAR(256)            NOT NULL, -- 手续费
    transaction_time      TIMESTAMP NULL,                   -- 交易时间
    block_height          VARCHAR(32) NULL,                 -- 块高
    notes                 TEXT NULL,                        -- 备注
    post_tx_count         INTEGER      DEFAULT 0  NOT NULL, -- 已发送交易次数
    post_confirm_tx_count INTEGER      DEFAULT 0  NOT NULL, -- 已确认交易次数
    err_code              INTEGER NULL, -- 发送交易错误吗
    err_msg               TEXT NULL,                        -- 发送交易错误日志
<<<<<<< HEAD:wallet-database/schema/migrations/20250828094953_api_collect.sql
    
    -- ===== Order ACK（接单事实）=====
    order_ack_sent_at     TIMESTAMP NULL,                   -- 确认已接收并持久化该订单
    
    -- ===== Build / Broadcast Execution Facts =====
    building_at           TIMESTAMP NULL,                   -- BuildTx 执行占位
    last_broadcast_at     TIMESTAMP NULL,                   -- 最近一次 Broadcast 执行占位
    
    -- ===== Result ACK（结果确认事实）=====
    result_ack_sent_at    TIMESTAMP NULL,                   -- 确认已将链上结果可靠告知后端
    result_ack_send_count INTEGER      DEFAULT 0  NOT NULL, -- Result ACK 发送次数
    
    -- ===== Terminal Fact =====
    finished_at           TIMESTAMP NULL,                   -- 链上终态事实
    
=======
    tx_ack_sent_at        TIMESTAMP NULL,                   -- 交易确认发送时间
    tx_res_ack_sent_at    TIMESTAMP NULL,                   -- 交易结果确认发送时间
>>>>>>> ca4b1c2f2dd8305abbb165a1a59cfba227194dda:wallet-database/schema/api_funds/migrations/20250828094953_api_collect.sql
    created_at            TIMESTAMP               NOT NULL,
    updated_at            TIMESTAMP
);
CREATE INDEX api_collect_from ON api_collect (uid, from_addr);
CREATE UNIQUE INDEX api_collect_trade_no ON api_collect (trade_no);
-- 此索引用于加速查询未发送 ACK 的collect记录，提高幂等性检查效率
CREATE INDEX api_collect_ack_times ON api_collect (tx_ack_sent_at, tx_res_ack_sent_at);