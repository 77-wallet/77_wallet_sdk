use crate::request::{api_wallet::trans::ApiBaseTransferReq, transaction::Signer};

#[derive(Debug, Clone)]
pub struct ApiTransferExReq {
    pub base: ApiBaseTransferReq,
    pub password: String,
    pub fee_setting: String,
    pub signer: Option<Signer>,
}
