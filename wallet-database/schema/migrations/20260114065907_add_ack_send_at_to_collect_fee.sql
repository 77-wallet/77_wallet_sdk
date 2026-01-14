-- Add migration script here
-- 1. 为 api_collect 表添加 Tx ACK 发送时间字段
ALTER TABLE api_collect
ADD COLUMN tx_ack_sent_at TIMESTAMP DEFAULT NULL;
-- 2. 为 api_collect 表添加 TxRes ACK 发送时间字段
ALTER TABLE api_collect
ADD COLUMN tx_res_ack_sent_at TIMESTAMP DEFAULT NULL;
-- 3. 为 api_fee 表添加 Tx ACK 发送时间字段
ALTER TABLE api_fee
ADD COLUMN tx_ack_sent_at TIMESTAMP DEFAULT NULL;
-- 4. 为 api_fee 表添加 TxRes ACK 发送时间字段
ALTER TABLE api_fee
ADD COLUMN tx_res_ack_sent_at TIMESTAMP DEFAULT NULL;
-- 6. （可选）创建索引以提高分析和失败重试扫描性能
-- 此索引用于加速查询未发送 ACK 的collect记录，提高幂等性检查效率
CREATE INDEX api_collect_ack_times ON api_collect (tx_ack_sent_at, tx_res_ack_sent_at);
-- 此索引用于加速查询未发送 ACK 的fee记录，提高幂等性检查效率
CREATE INDEX api_fee_ack_times ON api_fee (tx_ack_sent_at, tx_res_ack_sent_at);