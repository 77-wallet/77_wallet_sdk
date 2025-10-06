use crate::response_vo::chain::ChainList;

#[derive(Debug)]
pub struct AddCoinReq {
    pub wallet_address: String,
    pub account_id: u32,
    pub chain_list: ChainList,
}
#[derive(Debug)]
pub struct AddMultisigCoinReq {
    pub address: String,
    pub chain_list: ChainList,
}
