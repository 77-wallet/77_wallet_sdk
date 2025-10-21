-- Add migration script here
CREATE TABLE api_collect_strategy
(
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    uid              VARCHAR(20) NULL,                 -- 总钱包
    name             VARCHAR(64)             NOT NULL, -- 总钱包名称
    min_value        VARCHAR(64)             NOT NULL, -- 最小值
    idx INTEGER NOT NULL, -- 索引
    risk_idx       INTEGER             NOT NULL, -- 链码
    custom_addr    FLOAT             NOT NULL, -- 自定义地址
    created_at       TIMESTAMP               NOT NULL,
    updated_at       TIMESTAMP
);

CREATE UNIQUE INDEX api_collect_strategy_uid_idx ON api_collect_strategy (uid);
