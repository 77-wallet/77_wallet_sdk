use crate::request::transaction;
use wallet_chain_interact::ton::{
    operations::{token_transfer::TokenTransferOpt, transfer::TransferOpt, BuildInternalMsg},
    provider::Provider,
    Cell,
};
use wallet_types::chain::address::r#type::TonAddressType;
use wallet_utils::unit;

pub async fn build_ext_cell(
    req: &transaction::BaseTransferReq,
    provider: &Provider,
    address_type: TonAddressType,
) -> Result<Cell, crate::ServiceError> {
    if let Some(token) = req.token_address.clone() {
        let value = unit::convert_to_u256(&req.value, req.decimals)?;
        let arg = TokenTransferOpt::new(&req.from, &req.to, &token, value)?;

        Ok(arg.build_trans(address_type, provider).await?)
    } else {
        let arg = TransferOpt::new(&req.from, &req.to, &req.value)?;

        Ok(arg.build_trans(address_type, provider).await?)
    }
}

// pub fn build_token_arg(
//     req: &transaction::BaseTransferReq,
//     token: &str,
// ) -> Result<TokenTransferOpt, crate::ServiceError> {
//     let value = unit::convert_to_u256(&req.value, req.decimals)?;
//     let arg = TokenTransferOpt::new(&req.from, &req.to, token, value)?;

//     Ok(arg)
// }

// pub fn build_args(req: &transaction::BaseTransferReq) -> Result<TransferOpt, crate::ServiceError> {
//     let arg = TransferOpt::new(&req.from, &req.to, &req.value)?;
//     Ok(arg)
// }
