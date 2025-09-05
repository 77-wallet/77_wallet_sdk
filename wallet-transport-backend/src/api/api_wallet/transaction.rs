use crate::{
    consts::endpoint::api_wallet::{
        TRANS_EVENT_ACK, TRANS_EXECUTE_COMPLETE, TRANS_SERVICE_FEE_TRANS,
    },
    request::api_wallet::transaction::*,
    response::BackendResponse,
};

use super::BackendApi;

impl BackendApi {
    // 归集打手续费记录上传
    pub async fn upload_service_fee_record(
        &self,
        req: &ServiceFeeUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post(TRANS_SERVICE_FEE_TRANS)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 交易执行回执上传
    pub async fn upload_tx_exec_receipt(
        &self,
        req: &TxExecReceiptUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post(TRANS_EXECUTE_COMPLETE)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 交易记录恢复
    pub async fn restore_transaction_records(
        &self,
        req: &RestoreTxRecordsReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 收到交易事件确认
    pub async fn trans_event_ack(
        &self,
        req: &TransEventAckReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post(TRANS_EVENT_ACK)
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }
}
