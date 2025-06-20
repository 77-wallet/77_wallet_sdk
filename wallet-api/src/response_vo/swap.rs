use super::account::BalanceInfo;

// 查询报价的响应
#[derive(serde::Serialize)]
pub struct QuoteResp {
    // 提供方
    pub supplier: String,

    // 预估的手续费
    pub fee: BalanceInfo,

    // 转换后的值
    pub from_token_price: String,

    // 滑点
    pub slippage: String,

    // 最小获得数量
    pub min_amount: String,

    // 兑换路径
    pub path: String,

    // (选择的流动性)流动性
    pub liquidity: String,
}

#[derive(serde::Serialize)]
pub struct SupportChain {
    pub chain_code: String,
    pub name: String,
}

#[derive(serde::Serialize)]
pub struct SwapTokenInfo {
    pub symbol: String,
    pub name: String,
    pub chain_code: String,
    pub decimals: u32,
    pub contract_address: String,
}
