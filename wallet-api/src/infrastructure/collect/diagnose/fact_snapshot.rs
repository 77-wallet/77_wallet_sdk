use wallet_database::entities::api_collect::ApiCollectEntity;

/// 事实掩码字段定义
pub const FACT_MASK_SCHEMA: &[(&str, u8)] = &[
    ("order_ack_sent_at", 0),
    ("raw_tx", 1),
    ("need_service_fee", 2),
    ("ever_needed_service_fee", 3),
    ("tx_fee_res_ack_sent_at", 4),
    ("last_broadcast_at", 5),
    ("transaction_time", 6),
    ("tx_exec_receipt_uploaded_at", 7),
    ("result_ack_sent_at", 8),
    ("service_fee_uploaded_at", 9),
    ("finished_at", 10),
    ("err_code", 11),
];

/// 生成事实快照压缩日志
/// 这是生产排查神器
pub fn dump_fact_snapshot(c: &ApiCollectEntity) -> String {
    format!(
        "ack={:?} raw={} fee={:?} ever_fee={} fee_ack={:?} \
         broadcast={:?} tx_time={:?} receipt={:?} result_ack={:?} \
         service_fee_up={:?} finished={:?} err={:?}",
        c.order_ack_sent_at.is_some(),
        c.raw_tx.is_some(),
        c.need_service_fee,
        c.ever_needed_service_fee,
        c.tx_fee_res_ack_sent_at.is_some(),
        c.last_broadcast_at.is_some(),
        c.transaction_time.is_some(),
        c.tx_exec_receipt_uploaded_at.is_some(),
        c.result_ack_sent_at.is_some(),
        c.service_fee_uploaded_at.is_some(),
        c.finished_at.is_some(),
        c.err_code.is_some(),
    )
}

/// 生成事实掩码（用于机器处理）
pub fn fact_mask(c: &ApiCollectEntity) -> (u64, u8) {
    const MASK_VERSION: u8 = 1;

    let mut mask = 0u64;

    // 每一位代表一个事实的存在性
    if c.order_ack_sent_at.is_some() {
        mask |= 1 << 0;
    }
    if c.raw_tx.is_some() {
        mask |= 1 << 1;
    }
    if c.need_service_fee == Some(true) {
        mask |= 1 << 2;
    }
    if c.ever_needed_service_fee {
        mask |= 1 << 3;
    }
    if c.tx_fee_res_ack_sent_at.is_some() {
        mask |= 1 << 4;
    }
    if c.last_broadcast_at.is_some() {
        mask |= 1 << 5;
    }
    if c.transaction_time.is_some() {
        mask |= 1 << 6;
    }
    if c.tx_exec_receipt_uploaded_at.is_some() {
        mask |= 1 << 7;
    }
    if c.result_ack_sent_at.is_some() {
        mask |= 1 << 8;
    }
    if c.service_fee_uploaded_at.is_some() {
        mask |= 1 << 9;
    }
    if c.finished_at.is_some() {
        mask |= 1 << 10;
    }
    if c.err_code.is_some() {
        mask |= 1 << 11;
    }

    (mask, MASK_VERSION)
}
