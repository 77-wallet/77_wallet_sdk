use super::{
    account::{BalanceInfo, BalanceNotTruncate},
    EstimateFeeResp,
};
use crate::{domain::chain::swap::calc_slippage, request::transaction::DexRoute};
use alloy::primitives::U256;
use rust_decimal::Decimal;
use wallet_transport_backend::api::swap::ApproveInfo;

// 查询报价的响应
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiQuoteResp {
    pub chain_code: String,
    // 输入
    pub amount_in: BalanceNotTruncate,
    pub amount_in_symbol: String,
    // 输出
    pub amount_out: BalanceNotTruncate,
    pub amount_out_symbol: String,
    // 输入和输出的价值差
    pub price_spread: f64,
    // 预估的手续费
    pub fee: EstimateFeeResp,
    // 资源的消耗
    pub consumer: String,
    // 转换后的值
    pub from_token_price: f64,
    // 自动滑点值
    pub default_slippage: f64,
    // 滑点
    pub slippage: f64,
    // 最小获得数量
    pub min_amount: String,
    // 兑换路径
    pub dex_route_list: Vec<DexRoute>,
    // 需要授权的数量
    pub need_approve_amount: String,
    // 已经授权的数量
    pub approve_amount: String,
}

impl ApiQuoteResp {
    pub fn new(
        chain_code: String,
        token_in_symbol: String,
        token_out_symbol: String,
        slippage: f64,
        default_slippage: f64,
        dex_route_list: Vec<DexRoute>,
        bal_in: BalanceNotTruncate,
        bal_out: BalanceNotTruncate,
    ) -> Self {
        let (rate, price_spread) = Self::calc_price_spread_and_rate(&bal_in, &bal_out);

        Self {
            chain_code,
            amount_in: bal_in,
            amount_in_symbol: token_in_symbol,
            amount_out: bal_out,
            amount_out_symbol: token_out_symbol,
            price_spread,
            fee: EstimateFeeResp::default(),
            from_token_price: rate,
            slippage,
            default_slippage,
            min_amount: "0".to_string(),
            dex_route_list,
            consumer: "".to_string(),
            need_approve_amount: "0".to_string(),
            approve_amount: "0".to_string(),
        }
    }

    pub fn set_amount_out(&mut self, amount: U256, decimals: u32) {
        let min_amount = calc_slippage(amount, self.slippage);

        self.min_amount =
            wallet_utils::unit::format_to_string(min_amount, decimals as u8).unwrap_or_default();
    }

    // amount out 计算  滑点
    pub fn set_dex_amount_out(&mut self) -> Result<(), crate::ServiceError> {
        for dex_route in self.dex_route_list.iter_mut() {
            let amount = wallet_utils::unit::u256_from_str(&dex_route.amount_out)?;
            dex_route.amount_out = calc_slippage(amount, self.slippage).to_string();
        }
        Ok(())
    }

    // 计算token_in 和token_out的价差,以及兑换比例
    pub fn calc_price_spread_and_rate(
        amount_in: &BalanceNotTruncate,
        amount_out: &BalanceNotTruncate,
    ) -> (f64, f64) {
        let rate = if amount_in.amount > Decimal::ZERO {
            wallet_utils::conversion::decimal_to_f64(&(amount_out.amount / amount_in.amount))
                .unwrap_or(0.0)
        } else {
            0.0
        };

        let spread = match (amount_in.fiat_value, amount_out.fiat_value) {
            (Some(in_val), Some(out_val)) if in_val > 0.0 && out_val > 0.0 => {
                ((out_val - in_val) / in_val) * 100.0
            }
            _ => 0.0,
        };

        (rate, spread)
    }
}

// 授权列表
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveList {
    pub chain_code: String,
    pub token_address: String,
    pub symbol: String,
    pub spender: String,
    pub from: String,
    pub amount: String,
    pub limit_type: String,
    pub remaining_allowance: String,
}

impl From<ApproveInfo> for ApproveList {
    fn from(value: ApproveInfo) -> Self {
        Self {
            chain_code: value.chain_code,
            token_address: value.token_addr,
            from: value.owner_address,
            symbol: "".to_string(),
            spender: value.spender,
            amount: value.value.clone(),
            remaining_allowance: value.value,
            limit_type: value.limit_type,
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapTokenInfo {
    pub symbol: String,
    pub decimals: u32,
    pub chain_code: String,
    pub name: String,
    pub token_addr: String,
    pub balance: BalanceInfo,
}
