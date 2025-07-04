// tron 和 eth 系列的交易参数
use alloy::{
    primitives::{Address, U256},
    sol,
};

use crate::request::transaction::DexRoute;

// evm 系列调用合约的方法
sol!(
    #[derive(Debug)]
    struct DexRouterParam1 {
        SubDexRouterParam1[] subDexRouters;
        // uint256 amountIn;
        // uint256 minAmountOut;
    }

    #[derive(Debug)]
    struct SubDexRouterParam1 {
        uint16 dexId;
        address poolId;
        bool zeroForOne;
        uint256 amountIn;
        uint256 minAmountOut;
    }

    #[derive(Debug)]
    function dexSwap1(
        DexRouterParam1[] calldata routerParam,
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minAmountOut,
        address recipient,
        bool allowPartialFill
    ) external nonReentrant returns (uint256 usedAmountIn, uint256 amountOut);
);

//  聚合器合约参数
pub struct SwapParams {
    // 聚合器地址
    pub aggregator_addr: Address,
    // 转账金额
    pub amount_in: U256,
    // 最小输出
    pub min_amount_out: U256,
    // 接收地址
    pub recipient: Address,
    // 输入token
    pub token_in: Address,
    // 输出token
    pub token_out: Address,
    // 路由数据
    pub dex_router: Vec<DexRoute>,
    // 允许部分兑换
    pub allow_partial_fill: bool,
}

impl SwapParams {
    // eth 地址类型转为tron的地址类型
    pub fn aggregator_tron_addr(&self) -> Result<String, crate::ServiceError> {
        let address = self.aggregator_addr.to_string();

        Ok(wallet_utils::address::eth_addr_to_tron_addr(&address)?)
    }

    pub fn recipient_tron_addr(&self) -> Result<String, crate::ServiceError> {
        let address = self.recipient.to_string();

        Ok(wallet_utils::address::eth_addr_to_tron_addr(&address)?)
    }
}

impl TryFrom<&SwapParams> for dexSwap1Call {
    type Error = crate::ServiceError;

    fn try_from(value: &SwapParams) -> Result<Self, Self::Error> {
        use wallet_utils::{address::parse_eth_address, unit::u256_from_str};

        let mut router_param = Vec::with_capacity(value.dex_router.len());

        for quote in value.dex_router.iter() {
            let mut sub_routes = Vec::with_capacity(quote.route_in_dex.len());

            let amount_in = u256_from_str(&quote.amount_in)?;
            let amount_out = u256_from_str(&quote.amount_out)?;

            for pool in &quote.route_in_dex {
                let mut sub_route = SubDexRouterParam1 {
                    dexId: pool.dex_id,
                    poolId: parse_eth_address(&pool.pool_id)?,
                    zeroForOne: pool.zero_for_one,
                    amountIn: u256_from_str(&pool.amount_in)?,
                    minAmountOut: u256_from_str(&pool.min_amount_out)?,
                };

                if sub_routes.len() == 0 {
                    sub_route.amountIn = amount_in;
                    sub_route.minAmountOut = amount_out;
                }

                sub_routes.push(sub_route);
            }

            router_param.push(DexRouterParam1 {
                subDexRouters: sub_routes,
            });
        }

        Ok(dexSwap1Call {
            routerParam: router_param,
            tokenIn: value.token_in,
            tokenOut: value.token_out,
            amountIn: value.amount_in,
            minAmountOut: value.min_amount_out,
            recipient: value.recipient,
            allowPartialFill: value.allow_partial_fill,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DexRouterParam1, SubDexRouterParam1};
    use crate::domain::chain::swap::evm_swap::dexSwap1Call;
    use alloy::sol_types::SolCall;

    #[test]
    fn test_build_params() {
        let dex_router_param1 = SubDexRouterParam1 {
            dexId: 1,
            poolId: alloy::primitives::Address::default(),
            zeroForOne: true,
            amountIn: alloy::primitives::U256::from(1),
            minAmountOut: alloy::primitives::U256::from(1),
        };

        let dex_router_param = DexRouterParam1 {
            subDexRouters: vec![dex_router_param1],
            // amountIn: alloy::primitives::U256::from(1),
            // minAmountOut: alloy::primitives::U256::from(1),
        };

        let call_val = dexSwap1Call {
            routerParam: vec![dex_router_param],
            tokenIn: alloy::primitives::Address::default(),
            tokenOut: alloy::primitives::Address::default(),
            amountIn: alloy::primitives::U256::from(1),
            minAmountOut: alloy::primitives::U256::from(1),
            recipient: alloy::primitives::Address::default(),
            allowPartialFill: true,
        };

        println!("{:?}", call_val.abi_encode());
    }
}
