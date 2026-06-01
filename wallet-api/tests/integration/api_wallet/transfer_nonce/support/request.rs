use wallet_api::request::api_wallet::{trans::ApiBaseTransferReq, transfer::ApiTransferExReq};

use crate::harness::SMOKE_WALLET_PASSWORD;

pub(crate) const BNB_TO_ADDR: &str = "0x998522f928A37837Fa8d6743713170243b95f98a";

pub(crate) fn make_transfer_req(from: &str, to: &str) -> ApiTransferExReq {
    let mut base = ApiBaseTransferReq::new(from, to, "0.0000001", "bnb");
    base.with_token(None, 18, "BNB");
    ApiTransferExReq {
        base,
        password: SMOKE_WALLET_PASSWORD.to_string(),
        fee_setting: "".to_string(),
        signer: None,
    }
}
