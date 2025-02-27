use super::BackendApi;

pub struct PermissionAcceptReq {
    pub hash: String,
    pub tx_str: String,
    // upsert ,delete
    pub op_type: String,
}
impl BackendApi {
    pub async fn permission_accept(
        &self,
        _aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<(), crate::Error> {
        // let res = self
        //     .client
        //     .post("delegate/order")
        //     .json(req)
        //     .send::<BackendResponse>()
        //     .await?;
        // res.process(aes_cbc_cryptor)
        Ok(())
    }
}
