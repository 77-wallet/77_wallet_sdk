use super::account::BalanceInfo;
use crate::{domain::chain::swap::calc_slippage, request::transaction::DexRoute};
use alloy::primitives::U256;
use wallet_transport_backend::api::swap::ApproveInfo;

// 查询报价的响应
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiQuoteResp {
    // 输入
    pub amount_in: BalanceInfo,
    // 输出
    pub amount_out: BalanceInfo,
    // 输入和输出的价值差
    pub price_spread: f64,
    // 提供方
    pub supplier: String,
    // 预估的手续费
    pub fee: BalanceInfo,
    // 资源的消耗
    pub consumer: String,
    // 转换后的值
    pub from_token_price: f64,
    // 滑点
    pub slippage: f64,
    // 最小获得数量
    pub min_amount: String,
    // 兑换路径
    pub dex_route_list: Vec<DexRoute>,
    // // (选择的流动性)流动性
    // pub liquidity: String,
    // 需要授权的数量
    pub need_approve_amount: String,
}

impl ApiQuoteResp {
    pub fn new(
        slippage: f64,
        dex_route_list: Vec<DexRoute>,
        bal_in: BalanceInfo,
        bal_out: BalanceInfo,
    ) -> Self {
        let (rate, price_spread) = Self::calc_price_spread_and_rate(&bal_in, &bal_out);

        Self {
            amount_in: bal_in,
            amount_out: bal_out,
            price_spread,
            supplier: "77_DexAggreagre".to_string(),
            fee: BalanceInfo::default(),
            from_token_price: rate,
            slippage,
            min_amount: "0".to_string(),
            dex_route_list,
            // liquidity: "".to_string(),
            consumer: "".to_string(),
            need_approve_amount: "0".to_string(),
        }
    }

    pub fn set_amount_out(&mut self, amount: U256, decimals: u32) {
        let min_amount = calc_slippage(amount, self.slippage);

        self.min_amount =
            wallet_utils::unit::format_to_string(min_amount, decimals as u8).unwrap_or_default();
    }

    // 计算token_in 和token_out的价差,以及兑换比例
    pub fn calc_price_spread_and_rate(
        amount_in: &BalanceInfo,
        amount_out: &BalanceInfo,
    ) -> (f64, f64) {
        if let Some(amount_out) = amount_out.fiat_value
            && amount_out > 0.0
        {
            if let Some(amount_in) = amount_in.fiat_value
                && amount_in > 0.0
            {
                let spread = (amount_out - amount_in) / amount_in;
                return (spread * 100.0, amount_out / amount_in);
            }
        }

        (0.0, 0.0)
    }
}

// 授权列表
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveList {
    pub chain_code: String,
    pub token_address: String,
    pub spender: String,
    pub amount: String,
    pub limit_type: String,
}

impl From<ApproveInfo> for ApproveList {
    fn from(value: ApproveInfo) -> Self {
        Self {
            chain_code: value.chain_code,
            token_address: value.token_addr,
            spender: value.spender,
            amount: value.value,
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
