use super::BackendApi;
use crate::{
    request::{SignedTranAcceptReq, SignedTranCreateReq, SignedTranUpdateHashReq, SyncBillReq},
    response::BackendResponse,
    response_vo::{chain::GasOracle, transaction::RecordResp},
};
use std::collections::HashMap;

impl BackendApi {
    pub async fn signed_tran_create(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedTranCreateReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("signed/trans/create")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_tran_accept(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedTranAcceptReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("signed/trans/accept")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_tran_update_trans_hash(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedTranUpdateHashReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("signed/trans/updateTransdHash")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn gas_oracle(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        chain_code: &str,
    ) -> Result<GasOracle, crate::Error> {
        let mut params = HashMap::new();
        params.insert("chainCode", chain_code);

        let res = self
            .client
            .post("token/findGasTracker")
            .json(params)
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn record_lists(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        chain_code: &str,
        address: &str,
        start_time: Option<String>,
    ) -> Result<RecordResp, crate::Error> {
        let req = SyncBillReq::new(chain_code, address, start_time);

        let res = self
            .client
            .post("account/record/list")
            .json(req)
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }
}
