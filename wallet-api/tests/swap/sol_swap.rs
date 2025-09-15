use crate::get_manager;
use wallet_api::request::transaction::{DexRoute, QuoteReq, SwapReq, SwapTokenInfo};

// Ethereum aggregator address
const AGGREGATOR: &str = "FqhFY3ycmuif2T7YJZDwytyvrxEhnr6MYV4k2qxENkVH";

const RECIPIENT: &str = "78JSPvcz3CcwaACJsdgW6PSj6Vyu8quPHcNuerJy5DGx";

fn token_in_out() -> (SwapTokenInfo, SwapTokenInfo) {
    let token_in = SwapTokenInfo {
        token_addr: "So11111111111111111111111111111111111111112".to_string(),
        symbol: "sol".to_string(),
        decimals: 9,
    };

    let token_out = SwapTokenInfo {
        token_addr: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
        symbol: "WSOL".to_string(),
        decimals: 6,
    };

    (token_in, token_out)
}

#[tokio::test]
async fn test_quote() {
    let wallet_manager = get_manager().await;

    // WSOL  So11111111111111111111111111111111111111112

    let (token_in, token_out) = token_in_out();

    let req = QuoteReq {
        aggregator_addr: AGGREGATOR.to_string(),
        recipient: RECIPIENT.to_string(),
        chain_code: "sol".to_string(),
        amount_in: "0.001".to_string(),
        token_in,
        token_out,
        dex_list: vec![3],
        slippage: None,
        allow_partial_fill: false,
    };

    let result = wallet_manager.quote(req).await;
    tracing::warn!("quote = {}", serde_json::to_string(&result).unwrap());
}

// (选择的流动性)流动性
#[tokio::test]
async fn test_swap() {
    let wallet_manager = get_manager().await;

    let (token_in, token_out) = token_in_out();
    let o_value = "0.001";

    let dex_route_str = r#"[{"amountIn":"1000000","amountOut":"242711","percentage":"10000","routeInDex":[{"dexId":3,"poolId":"6MUjnGffYaqcHeqv4nNemUQVNMpJab3W2NV9bfPj576c","inTokenSymbol":"WSOL","inTokenAddr":"So11111111111111111111111111111111111111112","outTokenSymbol":"USDC","outTokenAddr":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v","zeroForOne":true,"fee":"200","amountIn":"0","minAmountOut":"0"},{"dexId":3,"poolId":"BZtgQEyS6eXUXicYPHecYQ7PybqodXQMvkjUbP4R8mUU","inTokenSymbol":"USDC","inTokenAddr":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v","outTokenSymbol":"USDT","outTokenAddr":"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB","zeroForOne":true,"fee":"1","amountIn":"0","minAmountOut":"0"}]}]"#;

    let dex_router: Vec<DexRoute> =
        wallet_utils::serde_func::serde_from_str(dex_route_str).unwrap();

    let req = SwapReq {
        aggregator_addr: AGGREGATOR.to_string(),
        amount_in: o_value.to_string(),
        amount_out: "0.24".to_string(),
        min_amount_out: "0.23".to_string(),
        chain_code: "sol".to_string(),
        recipient: RECIPIENT.to_string(),
        token_in,
        token_out,
        dex_router,
        allow_partial_fill: false,
    };

    let fee = r#"{"base_fee":5000,"priority_fee_per_compute_unit":null,"compute_units_consumed":100000,"extra_fee":null}"#.to_string();
    let password = "123456".to_string();

    let result = wallet_manager.swap(req, fee, password).await;
    tracing::warn!("swap hash = {}", serde_json::to_string(&result).unwrap());
}
