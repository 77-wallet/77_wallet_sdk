pub mod evm_swap;

// 波场聚合器合约地址
pub const TRON_SWAP_ADDRESS: &str = "TJ8rG2LZ6wQJgPp7sQ5XZzvWQqQa4pXQVW";
// eth 合约地址
pub const ETH_SWAP_ADDRESS: &str = "0x59a4ad52B1dEfC42033f8f109a7cF53924296112";

#[derive(Debug, serde::Serialize)]
pub struct EstimateSwapResult {
    pub amount_in: alloy::primitives::U256,
    pub amount_out: alloy::primitives::U256,
    // 手续费,除了精度的。
    pub fee: f64,
    // 消耗的资源(bsc gas)
    pub consumer: String,
}
