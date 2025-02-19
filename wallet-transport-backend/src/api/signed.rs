use serde_json::json;

use super::BackendApi;
use crate::{
    response::BackendResponse,
    response_vo::{
        address::AddressUidList,
        multisig::{DepositAddress, MultisigServiceFees},
    },
    FindAddressRawDataRes, MultisigAccountIsCancelRes, SignedCreateOrderReq, SignedOrderAcceptReq,
    SignedSaveAddressReq, SignedUpdateRechargeHashReq, SignedUpdateSignedHashReq,
    SingedOrderCancelReq,
};

impl BackendApi {
    pub async fn address_find_address_raw_data(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::FindAddressRawDataReq,
    ) -> Result<FindAddressRawDataRes, crate::Error> {
        let res = self
            .client
            .post("signed/order/findAddressRawData")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_find_address(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::SignedFindAddressReq,
    ) -> Result<DepositAddress, crate::Error> {
        let res = self
            .client
            .post("signed/order/findAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_fee_list(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::SignedFeeListReq,
    ) -> Result<MultisigServiceFees, crate::Error> {
        let res = self
            .client
            .post("/signed/order/feeList")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_update_signed_hash(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedUpdateSignedHashReq,
    ) -> Result<Option<String>, crate::Error> {
        let res = self
            .client
            .post("/signed/order/updateSignedHash")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_update_recharge_hash(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedUpdateRechargeHashReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("/signed/order/updateRechargeHash")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_save_confirm_address(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: SignedSaveAddressReq,
    ) -> Result<Option<String>, crate::Error> {
        let res = self
            .client
            .post("/signed/order/saveConfirmAddress")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn get_address_uid(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        chain_code: String,
        address: Vec<String>,
    ) -> Result<AddressUidList, crate::Error> {
        let req = json!({
            "addressList": address,
            "chainCode": chain_code
        });

        let res = self
            .client
            .post("/keys/queryUidByAddress")
            .json(req)
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_cancel(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SingedOrderCancelReq,
    ) -> Result<Option<String>, crate::Error> {
        let res = self
            .client
            .post("/signed/order/cancel")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_create(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: SignedCreateOrderReq,
    ) -> Result<String, crate::Error> {
        let res = self
            .client
            .post("/signed/order/create")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn signed_order_accept(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &SignedOrderAcceptReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("/signed/order/accept")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    // /signed/order/success
    pub async fn signed_order_success(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: SignedUpdateRechargeHashReq,
    ) -> Result<String, crate::Error> {
        let res = self
            .client
            .post("/signed/order/accept")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    // cancel multisig queue
    pub async fn signed_trans_cancel(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        queue_id: &str,
        raw_data: String,
    ) -> Result<(), crate::Error> {
        let req = serde_json::json!({
            "withdrawId":queue_id.to_string(),
            "rawData":raw_data
        });

        self.post_request::<_, ()>("/signed/trans/cancel", req, aes_cbc_cryptor)
            .await
    }

    // Update the raw data of the multisig account or queue.
    pub async fn update_raw_data(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        id: &str,
        raw_data: String,
    ) -> Result<(), crate::Error> {
        let req = serde_json::json!({
            "businessId":id.to_string(),
            "rawData":raw_data
        });

        self.post_request::<_, ()>("/signed/order/saveRawData", req, aes_cbc_cryptor)
            .await
    }

    pub async fn check_multisig_account_is_cancel(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        account_id: &str,
    ) -> Result<MultisigAccountIsCancelRes, crate::Error> {
        let req = serde_json::json!({
            "orderId":account_id.to_string(),
        });

        self.post_request::<_, MultisigAccountIsCancelRes>(
            "signed/order/findCancelStatusById",
            req,
            aes_cbc_cryptor,
        )
        .await
    }
}
