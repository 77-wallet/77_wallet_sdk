use crate::request::transaction::{BaseTransferReq, Signer};

#[derive(Debug, Clone)]
pub struct ApiTransferExReq {
    pub base: BaseTransferReq,
    pub password: String,
    pub fee_setting: String,
    pub signer: Option<Signer>,
}
