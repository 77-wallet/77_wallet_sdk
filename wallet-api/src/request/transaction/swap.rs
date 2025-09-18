use crate::{
    domain::chain::swap::{SLIPPAGE, evm_swap::SwapParams, get_warp_address},
    infrastructure::swap_client::{AggQuoteRequest, DexId},
};
use alloy::primitives::U256;
use rand::Rng as _;
use wallet_database::entities::bill::{BillExtraSwap, NewBillEntity};
use wallet_types::{chain::chain::ChainCode, constant::chain_code};

#[derive(Debug, serde::Serialize)]
pub struct SwapTokenListReq {
    pub chain_code: String,
    pub search: String,
    pub exclude_token: Vec<String>,
    pub wallet_address: String,
    pub account_id: i32,
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
            exclude_tokens: value.exclude_token,
        }
    }
}

// deposit,和withdraw 统一认为swap
#[derive(Debug)]
pub enum SwapInnerType {
    Swap,
    Deposit,
    Withdraw,
}

pub struct DepositReq {
    pub from: String,
    pub amount: String,
    pub token: String,
    pub chain_code: String,
}

#[derive(Debug)]
pub struct WithdrawReq {
    pub from: String,
    pub amount: String,
    pub token: String,
    pub chain_code: String,
}

#[derive(Debug)]
pub struct ApproveReq {
    pub contract: String,
    pub from: String,
    pub spender: String,
    pub value: String,
    pub chain_code: String,
}
impl ApproveReq {
    pub const NORMAL: &'static str = "NORMAL";
    pub const UN_LIMIT: &'static str = "UN_LIMIT";

    pub fn get_approve_type(&self) -> &'static str {
        if self.value == "-1" || self.value == "0" { Self::UN_LIMIT } else { Self::NORMAL }
    }

    pub fn get_value(&self, decimals: u8) -> Result<U256, crate::error::service::ServiceError> {
        if self.value == "-1" || self.value == "0" {
            Ok(U256::MAX)
        } else {
            Ok(wallet_utils::unit::convert_to_u256(&self.value, decimals)?)
        }
    }
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
    type Error = crate::error::service::ServiceError;

    fn try_from(value: &SwapReq) -> Result<Self, Self::Error> {
        let amount_in =
            wallet_utils::unit::convert_to_u256(&value.amount_in, value.token_in.decimals as u8)?;

        let min_amount_out = wallet_utils::unit::convert_to_u256(
            &value.min_amount_out,
            value.token_out.decimals as u8,
        )?;

        if value.chain_code == chain_code::ETHEREUM || value.chain_code == chain_code::BNB {
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
    type Error = crate::error::service::ServiceError;

    fn try_from(value: SwapReq) -> Result<Self, Self::Error> {
        let amount_in = wallet_utils::unit::string_to_f64(&value.amount_in)?;
        let amount_out = wallet_utils::unit::string_to_f64(&value.amount_out)?;

        let extra = BillExtraSwap {
            from_token_symbol: value.token_in.symbol.clone(),
            from_token_address: value.token_in.token_addr.clone(),
            from_token_amount: amount_in,
            to_token_symbol: value.token_out.symbol.clone(),
            to_token_address: value.token_out.token_addr.clone(),
            to_token_amount: amount_out,
            price: amount_out / amount_in,
        };

        Ok(NewBillEntity {
            hash: "".to_string(),
            from: value.recipient,
            to: "".to_string(),
            token: None,
            value: wallet_utils::unit::string_to_f64(&value.amount_in)?,
            multisig_tx: false,
            symbol: value.token_in.symbol,
            chain_code: value.chain_code,
            tx_type: 0,
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
    pub fn eth_or_weth(address: &str, chain_code: ChainCode) -> String {
        if address.is_empty() {
            get_warp_address(chain_code).unwrap().to_string()
        } else {
            address.to_string()
        }
    }

    pub fn amount_in_u256(&self) -> Result<U256, crate::error::service::ServiceError> {
        Ok(wallet_utils::unit::convert_to_u256(&self.amount_in, self.token_in.decimals as u8)?)
    }

    pub fn recipient_address(
        &self,
    ) -> Result<alloy::primitives::Address, crate::error::service::ServiceError> {
        Ok(wallet_utils::address::parse_eth_address(&self.recipient)?)
    }

    pub fn aggregator_address(
        &self,
    ) -> Result<alloy::primitives::Address, crate::error::service::ServiceError> {
        Ok(wallet_utils::address::parse_eth_address(&self.aggregator_addr)?)
    }

    // 波场的地址 转eth alloy Address type
    pub fn addr_tron_to_eth(
        addr: &str,
    ) -> Result<alloy::primitives::Address, crate::error::service::ServiceError> {
        let hex_addr = wallet_utils::address::bs58_addr_to_hex_bytes(addr)?;

        let hex_addr = hex::encode(&hex_addr[1..21]);

        Ok(wallet_utils::address::parse_eth_address(&hex_addr)?)
    }

    // 根据token地址判断是 进行那种swap交易
    pub fn swap_type(chain_code: ChainCode, token_in: &str, token_out: &str) -> SwapInnerType {
        let warp_coin = get_warp_address(chain_code).unwrap();

        if token_in.is_empty() && token_out == warp_coin {
            SwapInnerType::Deposit
        } else if token_in == warp_coin && token_out.is_empty() {
            SwapInnerType::Withdraw
        } else {
            SwapInnerType::Swap
        }
    }

    // 前端有传入的使用前端传入的，没有使用报价返回的。
    pub fn get_slippage(&self, default_slippage: u64) -> f64 {
        self.slippage.unwrap_or(default_slippage as f64 / SLIPPAGE)
    }
}

#[derive(Clone)]
pub struct SwapTokenInfo {
    pub symbol: String,
    pub decimals: u32,
    pub token_addr: String,
}

impl TryFrom<&QuoteReq> for AggQuoteRequest {
    type Error = crate::error::service::ServiceError;
    fn try_from(value: &QuoteReq) -> Result<Self, Self::Error> {
        let chain_code = ChainCode::try_from(value.chain_code.as_str())?;

        let amount =
            wallet_utils::unit::convert_to_u256(&value.amount_in, value.token_in.decimals as u8)?;

        let mut rng = rand::thread_rng();
        let n: u32 = rng.gen_range(0..1000);

        let unique =
            format!("{}_{}_{}", value.recipient, wallet_utils::time::now().timestamp(), n,);
        let dex_ids = value.dex_list.iter().map(|dex_id| DexId { dex_id: *dex_id }).collect();

        Ok(Self {
            chain_code: chain_code.to_string(),
            amount: amount.to_string(),
            unique,
            in_token_addr: QuoteReq::eth_or_weth(&value.token_in.token_addr, chain_code),
            out_token_addr: QuoteReq::eth_or_weth(&value.token_out.token_addr, chain_code),
            dex_ids,
        })
    }
}

// 路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
// #[serde(rename_all = "camelCase")]
pub struct DexRoute {
    #[serde(rename = "amountIn", alias = "amount_in")]
    pub amount_in: String,
    #[serde(rename = "amountOut", alias = "amount_out")]
    pub amount_out: String,
    pub percentage: String,
    #[serde(rename = "routeInDex", alias = "route_in_dex")]
    pub route_in_dex: Vec<RouteInDex>,
}

impl DexRoute {
    // 针对 deposit 和 withdraw 的路由
    pub fn new(amount: String, token_in: &SwapTokenInfo, token_out: &SwapTokenInfo) -> Self {
        let router = RouteInDex {
            dex_id: 0,
            pool_id: "".to_string(),
            in_token_symbol: token_in.symbol.clone(),
            in_token_addr: token_in.token_addr.clone(),
            out_token_symbol: token_out.symbol.clone(),
            out_token_addr: token_out.token_addr.clone(),
            zero_for_one: false,
            fee: "500".to_string(),
            amount_in: amount.clone(),
            min_amount_out: amount.clone(),
        };

        Self {
            amount_in: amount.clone(),
            amount_out: amount.clone(),
            percentage: "10000".to_string(),
            route_in_dex: vec![router],
        }
    }
}

// 子路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
// #[serde(rename_all = "camelCase")]
pub struct RouteInDex {
    #[serde(rename = "dexId", alias = "dex_id")]
    pub dex_id: u16,
    #[serde(rename = "poolId", alias = "pool_id")]
    pub pool_id: String,
    #[serde(rename = "inTokenSymbol", alias = "in_token_symbol")]
    pub in_token_symbol: String,
    #[serde(rename = "inTokenAddr", alias = "in_token_addr")]
    pub in_token_addr: String,
    #[serde(rename = "outTokenSymbol", alias = "out_token_symbol")]
    pub out_token_symbol: String,
    #[serde(rename = "outTokenAddr", alias = "out_token_addr")]
    pub out_token_addr: String,
    #[serde(rename = "zeroForOne", alias = "zero_for_one")]
    pub zero_for_one: bool,
    pub fee: String,
    #[serde(rename = "amountIn", alias = "amount_in")]
    pub amount_in: String,
    #[serde(rename = "minAmountOut", alias = "min_amount_out")]
    pub min_amount_out: String,
}
