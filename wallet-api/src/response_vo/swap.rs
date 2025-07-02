use super::account::BalanceInfo;
use crate::domain::{chain::swap::calc_slippage, swap_client::DexRoute};
use alloy::primitives::U256;

// 查询报价的响应
#[derive(serde::Serialize)]
pub struct ApiQuoteResp {
    pub amount_in: BalanceInfo,
    pub amount_out: BalanceInfo,
    // 提供方
    pub supplier: String,
    // 预估的手续费
    pub fee: BalanceInfo,
    // 资源的消耗
    pub consumer: String,
    // 转换后的值
    pub from_token_price: String,
    // 滑点
    pub slippage: f64,
    // 最小获得数量
    pub min_amount: String,
    // 兑换路径
    pub dex_route_list: Vec<DexRoute>,
    // (选择的流动性)流动性
    pub liquidity: String,
    // 需要授权的数量
    pub approve_amount: String,
}

impl ApiQuoteResp {
    pub fn new(
        price: String,
        slippage: f64,
        dex_route_list: Vec<DexRoute>,
        amount_in: BalanceInfo,
        amount_out: BalanceInfo,
    ) -> Self {
        Self {
            amount_in,
            amount_out,
            supplier: "77_DexAggreagre".to_string(),
            fee: BalanceInfo::default(),
            from_token_price: price,
            slippage,
            min_amount: "0".to_string(),
            dex_route_list,
            liquidity: "".to_string(),
            consumer: "".to_string(),
            approve_amount: "0".to_string(),
        }
    }

    pub fn set_amount_out(&mut self, amount: U256, decimals: u32) {
        let min_amount = calc_slippage(amount, self.slippage);

        self.min_amount =
            wallet_utils::unit::format_to_string(min_amount, decimals as u8).unwrap_or_default();
    }
}
