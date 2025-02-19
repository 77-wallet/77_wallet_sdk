use super::BackendApi;
use crate::{
    response::BackendResponse,
    response_vo::stake::{DelegateOrderArgs, DelegateQueryResp},
};

impl BackendApi {
    pub async fn delegate_order(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        account: &str,
        energy: i64,
    ) -> Result<DelegateQueryResp, crate::Error> {
        let req = DelegateOrderArgs {
            address: account.to_string(),
            energy_amount: energy,
        };

        let res = self
            .client
            .post("delegate/order")
            .json(req)
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn delegate_query_order(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        order_id: &str,
    ) -> Result<DelegateQueryResp, crate::Error> {
        let endpoint = format!("/delegate/order/{}", order_id);
        let res = self
            .client
            .post(&endpoint)
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn delegate_is_open(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<bool, crate::Error> {
        let res = self
            .client
            .post("delegate/isOpen")
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn delegate_complete(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        order_id: &str,
    ) -> Result<bool, crate::Error> {
        let endpoint = format!("/delegate/complete/{}", order_id);
        let res = self
            .client
            .post(&endpoint)
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn vote_list(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<crate::response_vo::stake::VoteListResp, crate::Error> {
        let res = self
            .client
            .post("vote/list")
            .send::<BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }
}
