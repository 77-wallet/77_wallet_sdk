use crate::{
    domain::chain::swap::{evm_swap::SwapParams, get_warp_address, SLIPPAGE},
    infrastructure::swap_client::{AggQuoteRequest, DexId},
};
use alloy::primitives::U256;
use wallet_database::entities::bill::{BillExtraSwap, NewBillEntity};
use wallet_types::{chain::chain::ChainCode, constant::chain_code};

#[derive(Debug, serde::Serialize)]
pub struct SwapTokenListReq {
    pub chain_code: String,
    pub search: String,
    pub exclude_token: Vec<String>,
    pub page_num: i64,
    pub page_size: i64,
}

impl From<SwapTokenListReq> for wallet_transport_backend::request::SwapTokenQueryReq {
    fn from(value: SwapTokenListReq) -> Self {
        Self {
            chain_code: value.chain_code,
            search: value.search,
            page_num: value.page_num,
            page_size: value.page_size,
        }
    }
}

#[derive(Debug)]
pub struct ApproveReq {
    pub contract: String,
    pub from: String,
    pub spender: String,
    pub value: String,
    pub chain_code: String,
    // NORMAL , UN_LIMIT(无限制)
    pub approve_type: String,
}
impl ApproveReq {
    // pub const NORMAL: &'static str = "NORMAL";
    pub const UN_LIMIT: &'static str = "UN_LIMIT";
}

#[derive(Debug)]
pub struct ApproveCancel {
    pub contract: String,
    pub from: String,
    pub spender: String,
    pub chain_code: String,
}

impl From<ApproveReq> for NewBillEntity {
    fn from(value: ApproveReq) -> Self {
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
            extra: None,
        }
    }
}

pub struct SwapReq {
    pub aggregator_addr: String,
    // 链code
    pub chain_code: String,
    // 输出金额
    pub amount_in: String,
    // 输出金额
    pub amount_out: String,
    // 接受最小的输出金额
    pub min_amount_out: String,
    // 接收地址
    pub recipient: String,
    // 输入token
    pub token_in: SwapTokenInfo,
    // 输出token
    pub token_out: SwapTokenInfo,
    // 路由数据
    pub dex_router: Vec<DexRoute>,
    // 允许部分兑换
    pub allow_partial_fill: bool,
}

impl TryFrom<&SwapReq> for SwapParams {
    type Error = crate::ServiceError;

    fn try_from(value: &SwapReq) -> Result<Self, Self::Error> {
        let amount_in =
            wallet_utils::unit::convert_to_u256(&value.amount_in, value.token_in.decimals as u8)?;

        let min_amount_out = wallet_utils::unit::convert_to_u256(
            &value.min_amount_out,
            value.token_out.decimals as u8,
        )?;

        if value.chain_code == chain_code::ETHEREUM {
            Ok(SwapParams {
                aggregator_addr: wallet_utils::address::parse_eth_address(&value.aggregator_addr)?,
                amount_in,
                min_amount_out,
                recipient: wallet_utils::address::parse_eth_address(&value.recipient)?,
                dex_router: value.dex_router.clone(),
                token_in: SwapParams::eth_parse_or_zero_addr(&value.token_in.token_addr)?,
                token_out: SwapParams::eth_parse_or_zero_addr(&value.token_out.token_addr)?,
                allow_partial_fill: value.allow_partial_fill,
            })
        } else {
            Ok(SwapParams {
                aggregator_addr: QuoteReq::addr_tron_to_eth(&value.aggregator_addr)?,
                amount_in,
                min_amount_out,
                recipient: QuoteReq::addr_tron_to_eth(&value.recipient)?,
                dex_router: value.dex_router.clone(),
                token_in: SwapParams::tron_parse_or_zero_addr(&value.token_in.token_addr)?,
                token_out: SwapParams::tron_parse_or_zero_addr(&value.token_out.token_addr)?,
                allow_partial_fill: value.allow_partial_fill,
            })
        }
    }
}

impl TryFrom<SwapReq> for NewBillEntity<BillExtraSwap> {
    type Error = crate::ServiceError;

    fn try_from(value: SwapReq) -> Result<Self, Self::Error> {
        let amount_in = wallet_utils::unit::string_to_f64(&value.amount_in)?;
        let amount_out = wallet_utils::unit::string_to_f64(&value.amount_out)?;

        let extra = BillExtraSwap {
            from_token_symbol: value.token_in.symbol.clone(),
            from_token_amount: amount_in,
            to_token_symbol: value.token_out.symbol.clone(),
            to_token_amount: amount_out,
            supplier: "77_wallet".to_string(),
            price: amount_out / amount_in,
        };

        Ok(NewBillEntity {
            hash: "".to_string(),
            from: value.recipient,
            to: "".to_string(),
            token: None,
            value: 0.0,
            multisig_tx: false,
            symbol: value.token_in.symbol,
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
            extra: Some(extra),
        })
    }
}

// 前端请求报价的参数
pub struct QuoteReq {
    // 聚合器提供的地址
    pub aggregator_addr: String,
    pub recipient: String,
    pub chain_code: String,
    // 未处理精度的值
    pub amount_in: String,
    pub token_in: SwapTokenInfo,
    pub token_out: SwapTokenInfo,
    // 选择的池子
    pub dex_list: Vec<i32>,
    // 滑点
    pub slippage: Option<f64>,
    // 允许部分兑换
    pub allow_partial_fill: bool,
}
impl QuoteReq {
    pub fn token_in_or_warp(&self, chain_code: ChainCode) -> String {
        if self.token_in.token_addr.is_empty() {
            get_warp_address(chain_code).unwrap().to_string()
        } else {
            self.token_in.token_addr.clone()
        }
    }

    pub fn amount_in_u256(&self) -> Result<U256, crate::ServiceError> {
        Ok(wallet_utils::unit::convert_to_u256(
            &self.amount_in,
            self.token_in.decimals as u8,
        )?)
    }

    pub fn recipient_address(&self) -> Result<alloy::primitives::Address, crate::ServiceError> {
        Ok(wallet_utils::address::parse_eth_address(&self.recipient)?)
    }

    pub fn aggregator_address(&self) -> Result<alloy::primitives::Address, crate::ServiceError> {
        Ok(wallet_utils::address::parse_eth_address(
            &self.aggregator_addr,
        )?)
    }

    // 波场的地址 转eth alloy Address type
    pub fn addr_tron_to_eth(addr: &str) -> Result<alloy::primitives::Address, crate::ServiceError> {
        let hex_addr = wallet_utils::address::bs58_addr_to_hex_bytes(addr)?;

        let hex_addr = hex::encode(&hex_addr[1..21]);

        Ok(wallet_utils::address::parse_eth_address(&hex_addr)?)
    }

    pub fn get_slippage(&self, default_slippage: u64) -> f64 {
        self.slippage.unwrap_or(default_slippage as f64 / SLIPPAGE)
    }
}

pub struct SwapTokenInfo {
    pub symbol: String,
    pub decimals: u32,
    pub token_addr: String,
}

impl TryFrom<&QuoteReq> for AggQuoteRequest {
    type Error = crate::ServiceError;
    fn try_from(value: &QuoteReq) -> Result<Self, Self::Error> {
        let chain_code = ChainCode::try_from(value.chain_code.as_str())?;

        let amount =
            wallet_utils::unit::convert_to_u256(&value.amount_in, value.token_in.decimals as u8)?;

        let dex_ids = value
            .dex_list
            .iter()
            .map(|dex_id| DexId { dex_id: *dex_id })
            .collect();

        Ok(Self {
            chain_code: chain_code.to_string(),
            amount: amount.to_string(),
            in_token_addr: value.token_in_or_warp(chain_code),
            out_token_addr: value.token_out.token_addr.clone(),
            dex_ids,
        })
    }
}

// 路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct DexRoute {
    pub amount_in: String, // 可选择转换为 U256
    pub amount_out: String,
    pub route_in_dex: Vec<RouteInDex>,
}

// 子路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct RouteInDex {
    pub dex_id: u16,
    pub pool_id: String,
    pub in_token_addr: String,
    pub out_token_addr: String,
    pub zero_for_one: bool,
    pub fee: String,
    pub amount_in: String,
    pub min_amount_out: String,
}
