use crate::request::api_wallet::transaction::*;

use super::BackendApi;

impl BackendApi {
    // 交易记录恢复
    pub async fn restore_transaction_records(
        &self,
        req: &RestoreTxRecordsReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 消息接收回执上传
    pub async fn upload_msg_receipt(
        &self,
        req: &MsgReceiptUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 交易执行回执上传
    pub async fn upload_tx_exec_receipt(
        &self,
        req: &TxExecReceiptUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 归集打手续费记录上传
    pub async fn upload_fee_collection_record(
        &self,
        req: &FeeCollectionUploadReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }
}
