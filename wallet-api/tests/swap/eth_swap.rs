use crate::get_manager;
use wallet_api::request::transaction::{
    ApproveReq, DexRoute, QuoteReq, RouteInDex, SwapReq, SwapTokenInfo,
};

// Ethereum aggregator address
const AGGREGATOR: &str = "0xb0635793E700E8A5AFbB94e12086E921FB0E3E3E";

#[tokio::test]
async fn test_default_quote() {
    let wallet_manager = get_manager().await;

    let chain_code = "eth".to_string();
    let token_in = "".to_string();
    // let token_out = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string();

    let resp = wallet_manager.default_quote(chain_code, token_in).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_support_chain() {
    let wallet_manager = get_manager().await;

    let resp = wallet_manager.chain_list().await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve() {
    let wallet_manager = get_manager().await;
    let params = ApproveReq {
        from: "0x998522f928A37837Fa8d6743713170243b95f98a".to_string(),
        spender: AGGREGATOR.to_string(),
        contract: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        value: "2".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.approve(params, password).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_fee() {
    let wallet_manager = get_manager().await;

    let params = ApproveReq {
        from: "0x998522f928A37837Fa8d6743713170243b95f98a".to_string(),
        spender: "0xb0635793E700E8A5AFbB94e12086E921FB0E3E3E".to_string(),
        contract: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        value: "0".to_string(),
        chain_code: "eth".to_string(),
    };

    let is_cancel = true;

    let resp = wallet_manager.approve_fee(params, is_cancel).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_list() {
    let wallet_manager = get_manager().await;

    let uid = "41028d217798181a73225cc57ad401a66da21c5b0853e83a50f377dffc75651d".to_string();
    let account_id = 1;

    let resp = wallet_manager.approve_list(uid, account_id).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_cancel() {
    let wallet_manager = get_manager().await;

    let params = ApproveReq {
        from: "0x998522f928A37837Fa8d6743713170243b95f98a".to_string(),
        spender: AGGREGATOR.to_string(),
        contract: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        value: "15".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.approve_cancel(params, password).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_quote() {
    let wallet_manager = get_manager().await;

    let token_in = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "wtrx".to_string(),
        decimals: 18,
    };

    let token_out = SwapTokenInfo {
        token_addr: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        symbol: "usdc".to_string(),
        decimals: 6,
    };

    // let token_in = SwapTokenInfo {
    //     token_addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
    //     symbol: "weth".to_string(),
    //     decimals: 18,
    // };
    // let token_out = SwapTokenInfo {
    //     token_addr: "".to_string(),
    //     symbol: "WETH".to_string(),
    //     decimals: 18,
    // };

    let req = QuoteReq {
        aggregator_addr: AGGREGATOR.to_string(),
        recipient: "0x998522f928A37837Fa8d6743713170243b95f98a".to_string(),
        chain_code: "eth".to_string(),
        amount_in: "0.001".to_string(),
        token_in,
        token_out,
        dex_list: vec![2, 3],
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

    let o_value = "5";

    let amount_in = wallet_utils::unit::convert_to_u256(&o_value, 6).unwrap();

    let dex_route1 = DexRoute {
        percentage: "10000".to_string(),
        amount_in: amount_in.to_string(),
        amount_out: "0".to_string(),
        route_in_dex: vec![
            RouteInDex {
                dex_id: 3,
                pool_id: "0xc7bbec68d12a0d1830360f8ec58fa599ba1b0e9b".to_string(),
                in_token_symbol: "USDT".to_string(),
                in_token_addr: "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
                out_token_addr: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                out_token_symbol: "WETH".to_string(),
                zero_for_one: false,
                fee: "100".to_string(),
                amount_in: 0.to_string(),
                min_amount_out: 0.to_string(),
            },
            RouteInDex {
                dex_id: 3,
                pool_id: "0xe0554a476a092703abdb3ef35c80e0d76d32939f".to_string(),
                in_token_symbol: "USDT".to_string(),
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

    let token_in = SwapTokenInfo {
        token_addr: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        symbol: "USDT".to_string(),
        decimals: 6,
    };

    let token_out = SwapTokenInfo {
        token_addr: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".to_string(),
        symbol: "usdc".to_string(),
        decimals: 6,
    };

    let req = SwapReq {
        aggregator_addr: AGGREGATOR.to_string(),
        amount_in: o_value.to_string(),
        amount_out: "0.01".to_string(),
        min_amount_out: "0.01".to_string(),
        chain_code: "eth".to_string(),
        recipient: "0x998522f928A37837Fa8d6743713170243b95f98a".to_string(),
        token_in,
        token_out,
        dex_router: vec![dex_route1],
        allow_partial_fill: false,
    };

    let fee =
                r#"{"gasLimit":328431,"baseFee":"1032084119","priorityFee":"214990929","maxFeePerGas":"1600000000"}"#
                    .to_string();
    let password = "123456".to_string();

    let result = wallet_manager.swap(req, fee, password).await;
    tracing::warn!("swap hash = {}", serde_json::to_string(&result).unwrap());
}

#[test]
fn test_address_covert() {
    let address = "TNUC9Qb1rRpS5CbWLmNMxXBjyFoydXjWFR";
    let res = QuoteReq::addr_tron_to_eth(&address).unwrap();
    println!("eth address = {}", res);
}
