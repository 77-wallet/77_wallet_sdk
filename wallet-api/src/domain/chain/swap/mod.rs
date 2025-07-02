use wallet_types::chain::chain::ChainCode;
pub mod evm_swap;

// 各个链的主币地址
pub const W_ETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
pub const W_BNB: &str = "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c";
pub const W_TRON: &str = "TNUC9Qb1rRpS5CbWLmNMxXBjyFoydXjWFR";

pub fn get_warp_address(chain_code: ChainCode) -> Option<&'static str> {
    match chain_code {
        ChainCode::Ethereum => Some(W_ETH),
        ChainCode::BnbSmartChain => Some(W_BNB),
        ChainCode::Tron => Some(W_TRON),
        _ => None,
    }
}

// 滑点计算
// slippage: 0.01 min value
// 万分之一
pub fn calc_slippage(amount: alloy::primitives::U256, slippage: f64) -> alloy::primitives::U256 {
    let slippage_bps = (slippage * 10000.0).round() as u64;

    let numerator = alloy::primitives::U256::from(10_000u64 - slippage_bps);
    let denominator = alloy::primitives::U256::from(10_000u64);

    amount * numerator / denominator
}

#[derive(Debug, serde::Serialize)]
pub struct EstimateSwapResult {
    pub amount_in: alloy::primitives::U256,
    pub amount_out: alloy::primitives::U256,
    // 原始的值
    pub fee: alloy::primitives::U256,
    // 消耗的资源(bsc gas)
    pub consumer: String,
}
