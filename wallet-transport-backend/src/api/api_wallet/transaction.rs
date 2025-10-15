use wallet_ecdh::GLOBAL_KEY;
use crate::{
    consts::endpoint::api_wallet::{
        TRANS_EVENT_ACK, TRANS_EXECUTE_COMPLETE, TRANS_SERVICE_FEE_TRANS,
    },
    request::api_wallet::transaction::*
    ,
};

use crate::api::BackendApi;
use crate::api_request::ApiBackendRequest;
use crate::api_response::ApiBackendResponse;

impl BackendApi {
    // 归集打手续费记录上传
    pub async fn upload_service_fee_record(
        &self,
        req: &ServiceFeeUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        GLOBAL_KEY.is_exchange_shared_secret() ?;
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_SERVICE_FEE_TRANS)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;

        res.process(TRANS_SERVICE_FEE_TRANS)
    }

    // 交易执行回执上传
    pub async fn upload_tx_exec_receipt(
        &self,
        req: &TxExecReceiptUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_EXECUTE_COMPLETE)
            .json(serde_json::json!(req))
            .send::<ApiBackendResponse>()
            .await?;

        res.process(TRANS_EXECUTE_COMPLETE)
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
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_EVENT_ACK)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        res.process(TRANS_EVENT_ACK)
    }
}
