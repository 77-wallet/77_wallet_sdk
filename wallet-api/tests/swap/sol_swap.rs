use crate::get_manager;
use wallet_api::request::transaction::{DexRoute, QuoteReq, SwapReq, SwapTokenInfo};

// Ethereum aggregator address
const AGGREGATOR: &str = "FqhFY3ycmuif2T7YJZDwytyvrxEhnr6MYV4k2qxENkVH";

const RECIPIENT: &str = "78JSPvcz3CcwaACJsdgW6PSj6Vyu8quPHcNuerJy5DGx";

fn token_in_out() -> (SwapTokenInfo, SwapTokenInfo) {
    let token_in = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "SOL".to_string(),
        decimals: 9,
    };
    let token_out = SwapTokenInfo {
        token_addr: "UPTx1d24aBWuRgwxVnFmX4gNraj3QGFzL3QqBgxtWQG".to_string(),
        symbol: "USDT".to_string(),
        decimals: 9,
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
    let o_value = "0.1";

    let dex_route_str = r#"[{"amountIn":"100000","amountOut":"103382","percentage":"10000","routeInDex":[{"dexId":3,"poolId":"CztrCcLhgfazkBchMW7wXQL37AWQdBP1tQWHBR249neh","inTokenSymbol":"USDC","inTokenAddr":"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v","outTokenSymbol":"WSOL","outTokenAddr":"So11111111111111111111111111111111111111112","zeroForOne":false,"fee":"60","amountIn":"0","minAmountOut":"0"},{"dexId":3,"poolId":"GN3W5LnmeGt5caNhNQavcufGj2bZfRwry5UQrezukYiL","inTokenSymbol":"WSOL","inTokenAddr":"So11111111111111111111111111111111111111112","outTokenSymbol":"USDT","outTokenAddr":"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB","zeroForOne":true,"fee":"400","amountIn":"0","minAmountOut":"0"}]}]"#;

    let dex_router: Vec<DexRoute> =
        wallet_utils::serde_func::serde_from_str(dex_route_str).unwrap();

    let req = SwapReq {
        aggregator_addr: AGGREGATOR.to_string(),
        amount_in: o_value.to_string(),
        amount_out: "0.1".to_string(),
        min_amount_out: "0.08".to_string(),
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
