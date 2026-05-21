-- Add resource delegate/undelegate facts for platform and local resource tasks.

CREATE TABLE api_resource_delegation
(
    id                           INTEGER PRIMARY KEY AUTOINCREMENT,
    uid                          TEXT        NOT NULL DEFAULT '', -- API 钱包总钱包 UID
    source                       INTEGER     NOT NULL, -- 资源事实来源：1=platform 后端下发；2=local SDK 本地发起
    operation_type               INTEGER     NOT NULL, -- 资源动作：1=delegate 代理；2=undelegate 回收
    origin_trade_no              TEXT NULL, -- 原归集/提币交易号
    origin_trade_type            INTEGER NULL, -- 原交易类型，取 ApiTradeType 数值
    resource_trade_no            TEXT        NOT NULL, -- 资源任务号；platform 使用后端 tradeNo，local 使用 SDK 本地 UUID
    chain_code                   TEXT        NOT NULL DEFAULT 'tron', -- 当前资源任务链，第一阶段固定为 tron
    owner_address                TEXT        NOT NULL, -- 资源提供方地址
    receiver_address             TEXT        NOT NULL, -- 资源接收方地址
    resource_type                INTEGER     NOT NULL DEFAULT 1, -- 资源类型：0=bandwidth；1=energy
    native_amount                TEXT        NOT NULL DEFAULT '0', -- 链上代理 TRX 数量，来自 MQTT nativeValue；amount 保留资源数量 rscValue
    amount                       TEXT        NOT NULL DEFAULT '0', -- 资源数量，字符串保存避免精度/单位转换损失
    status                       INTEGER     NOT NULL DEFAULT 1, -- 资源任务阶段：1=pending，2=success，3=fail；执行推进仍以事实字段为准

    task_ack_sent_at             TIMESTAMP NULL, -- platform 资源任务接收 ACK 已发送时间；local 不使用
    building_at                  TIMESTAMP NULL, -- 链上交易构建占位，防止重复构建
    tx_hash                      TEXT NULL, -- 资源代理/回收链上交易哈希
    tx_status                    TEXT NULL, -- 链上执行状态视图，如 success / fail / uncertain
    tx_exec_receipt_uploaded_at  TIMESTAMP NULL, -- 链上执行回执已上传时间

    result_status                INTEGER NULL, -- 后端最终结果或本地最终结果状态：1=success，2=fail
    result_received_at           TIMESTAMP NULL, -- 已接收并持久化最终结果时间
    result_ack_sent_at           TIMESTAMP NULL, -- platform 最终结果 ACK 已发送时间；local 不使用
    result_payload               TEXT NULL, -- 原始最终结果 payload，便于排障和兼容字段回放

    fail_type                    INTEGER NULL, -- 后端 failType 原样落库，不直接面向用户展示
    err_code                     TEXT NULL, -- 后端或 SDK 内部错误码原样落库
    err_msg                      TEXT NULL, -- 后端或 SDK 内部错误信息原样落库

    recover_status               INTEGER NULL, -- 本地回收恢复状态枚举：1=recover_waiting, 2=retry_build, 3=retry_recover
    next_retry_at                TIMESTAMP NULL, -- 下次恢复/重试时间
    retry_count                  INTEGER     NOT NULL DEFAULT 0, -- 恢复/重试次数

    created_at                   TIMESTAMP   NOT NULL, -- 创建时间
    updated_at                   TIMESTAMP NULL -- 更新时间
);

CREATE UNIQUE INDEX api_resource_delegation_trade_no
    ON api_resource_delegation (resource_trade_no);
CREATE INDEX api_resource_delegation_origin
    ON api_resource_delegation (origin_trade_no, origin_trade_type, source);
CREATE INDEX api_resource_delegation_scan
    ON api_resource_delegation (source, operation_type, status, next_retry_at);
CREATE INDEX api_resource_delegation_owner_receiver
    ON api_resource_delegation (owner_address, receiver_address, resource_type);
