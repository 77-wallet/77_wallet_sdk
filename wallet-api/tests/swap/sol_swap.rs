use crate::get_manager;
use wallet_api::request::transaction::{DexRoute, QuoteReq, RouteInDex, SwapReq, SwapTokenInfo};

// Ethereum aggregator address
const AGGREGATOR: &str = "0xb0635793E700E8A5AFbB94e12086E921FB0E3E3E";

fn token_in_out() -> (SwapTokenInfo, SwapTokenInfo) {
    let token_in = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "sol".to_string(),
        decimals: 9,
    };

    let token_out = SwapTokenInfo {
        token_addr: "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
        symbol: "USDT".to_string(),
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
        recipient: "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_string(),
        chain_code: "sol".to_string(),
        amount_in: "0.1".to_string(),
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

    let amount_in = wallet_utils::unit::convert_to_u256(&o_value, 6).unwrap();

    let dex_route1 = DexRoute {
        percentage: "10000".to_string(),
        amount_in: amount_in.to_string(),
        amount_out: "0".to_string(),
        route_in_dex: vec![
            // RouteInDex {
            //     dex_id: 3,
            //     pool_id: "0xc7bbec68d12a0d1830360f8ec58fa599ba1b0e9b".to_string(),
            //     in_token_symbol: "USDT".to_string(),
            //     in_token_addr: "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
            //     out_token_addr: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
            //     out_token_symbol: "WETH".to_string(),
            //     zero_for_one: false,
            //     fee: "100".to_string(),
            //     amount_in: 0.to_string(),
            //     min_amount_out: 0.to_string(),
            // },
            RouteInDex {
                dex_id: 3,
                pool_id: "0xe0554a476a092703abdb3ef35c80e0d76d32939f".to_string(),
                in_token_symbol: "sol".to_string(),
                in_token_addr: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                out_token_addr: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                out_token_symbol: "WETH".to_string(),
                zero_for_one: false,
                fee: "100".to_string(),
                amount_in: 0.to_string(),
                min_amount_out: 0.to_string(),
            },
        ],
    };

    let req = SwapReq {
        aggregator_addr: AGGREGATOR.to_string(),
        amount_in: o_value.to_string(),
        amount_out: "0.001".to_string(),
        min_amount_out: "0.001".to_string(),
        chain_code: "sol".to_string(),
        recipient: "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_string(),
        token_in,
        token_out,
        dex_router: vec![dex_route1],
        allow_partial_fill: false,
    };

    let fee = r#"{"base_fee":5000,"priority_fee_per_compute_unit":null,"compute_units_consumed":100000,"extra_fee":null}"#.to_string();
    let password = "123456".to_string();

    let result = wallet_manager.swap(req, fee, password).await;
    tracing::warn!("swap hash = {}", serde_json::to_string(&result).unwrap());
}
