use crate::domain::{
    chain::swap::evm_swap::SwapParams,
    swap_client::{DexId, DexRoute, QuoteRequest},
};
use wallet_database::entities::bill::NewBillEntity;

#[derive(Debug)]
pub struct ApproveParams {
    pub contract: String,
    pub from: String,
    pub spender: String,
    pub value: String,
    pub chain_code: String,
}

impl From<ApproveParams> for NewBillEntity {
    fn from(value: ApproveParams) -> Self {
        NewBillEntity {
            hash: "".to_string(),
            from: value.from,
            to: value.spender,
            token: Some(value.contract),
            value: wallet_utils::unit::string_to_f64(&value.value).unwrap(),
            multisig_tx: false,
            symbol: "".to_string(),
            chain_code: value.chain_code,
            tx_type: 1,
            tx_kind: wallet_database::entities::bill::BillKind::Approve,
            status: 1,
            queue_id: "".to_owned(),
            notes: "".to_string(),
            transaction_fee: "0".to_string(),
            resource_consume: "".to_string(),
            transaction_time: 0,
            block_height: "0".to_string(),
            signer: vec![],
        }
    }
}

#[derive(Debug)]
pub struct DepositParams {
    pub contract: String,
    pub from: String,
    pub value: String,
    pub chain_code: String,
}

pub struct SwapReq {
    pub chain_code: String,
    // 接收地址
    pub recipient: String,
    // 输入token
    pub token_in: String,
    // 输出token
    pub token_out: String,
    // 路由数据
    pub dex_router: Vec<DexRoute>,
    // 允许部分兑换
    pub allow_partial_fill: bool,
}

impl TryFrom<&SwapReq> for SwapParams {
    type Error = crate::ServiceError;

    fn try_from(value: &SwapReq) -> Result<Self, Self::Error> {
        Ok(SwapParams {
            recipient: wallet_utils::address::parse_eth_address(&value.recipient)?,
            dex_router: value.dex_router.clone(),
            token_in: wallet_utils::address::parse_eth_address(&value.token_in)?,
            token_out: wallet_utils::address::parse_eth_address(&value.token_out)?,
            allow_partial_fill: value.allow_partial_fill,
        })
    }
}

impl From<SwapReq> for NewBillEntity {
    fn from(value: SwapReq) -> Self {
        NewBillEntity {
            hash: "".to_string(),
            from: value.recipient,
            to: "".to_string(),
            token: None,
            value: 0.0,
            multisig_tx: false,
            symbol: "".to_string(),
            chain_code: value.chain_code,
            tx_type: 1,
            tx_kind: wallet_database::entities::bill::BillKind::Swap,
            status: 1,
            queue_id: "".to_owned(),
            notes: "".to_string(),
            transaction_fee: "0".to_string(),
            resource_consume: "".to_string(),
            transaction_time: 0,
            block_height: "0".to_string(),
            signer: vec![],
        }
    }
}

pub struct QuoteReq {
    pub from: String,
    pub chain_code: String,
    pub amount_in: String,
    pub token_in: String,
    pub token_out: String,
    pub from_symbol: String,
    // 选择的池子
    pub dex_list: Vec<i32>,
    // 滑点
    pub slippage: f64,
}

impl From<&QuoteReq> for QuoteRequest {
    fn from(value: &QuoteReq) -> Self {
        //TODO
        let chain_id = 1;

        let dex_ids = value
            .dex_list
            .iter()
            .map(|dex_id| DexId { dex_id: *dex_id })
            .collect();

        Self {
            chain_id,
            amount: value.amount_in.clone(),
            in_token_addr: value.token_in.clone(),
            out_token_addr: value.token_out.clone(),
            dex_ids,
        }
    }
}
