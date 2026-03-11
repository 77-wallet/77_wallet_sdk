use wallet_database::entities::api_collect::ApiCollectEntity;
use wallet_transport_backend::request::api_wallet::transaction::{
    TransStatus, TransType, TxExecReceiptUploadReq,
};

pub fn build_collect_tx_exec_receipt_payload(
    req: &ApiCollectEntity,
    trade_no: &str,
) -> TxExecReceiptUploadReq {
    let upload_status = if req.transaction_time.is_some() || req.last_broadcast_at.is_some() {
        TransStatus::Success
    } else {
        TransStatus::Fail
    };

    let remark = if matches!(upload_status, TransStatus::Success) || req.err_msg.is_empty() {
        ""
    } else {
        &req.err_msg
    };

    TxExecReceiptUploadReq::new(
        Some(&req.from_addr),
        Some(&req.to_addr),
        trade_no,
        TransType::Col,
        req.tx_hash.as_deref(),
        upload_status,
        remark,
    )
}
