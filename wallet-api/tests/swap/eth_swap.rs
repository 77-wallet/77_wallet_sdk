use crate::get_manager;
use wallet_api::request::transaction::{
    ApproveReq, DexRoute, QuoteReq, RouteInDex, SwapReq, SwapTokenInfo,
};

// Ethereum aggregator address
const AGGREGATOR: &str = "0xA36B5Fec0E93d24908fAA9966535567E9f888994";

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
        from: "0xD5359AD68FDb8a72386aB26C68D7648D548ec70a".to_string(),
        spender: AGGREGATOR.to_string(),
        contract: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        value: "400".to_string(),
        chain_code: "eth".to_string(),
        approve_type: "NORMAL".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.approve(params, password).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_fee() {
    let wallet_manager = get_manager().await;

    let params = ApproveReq {
        from: "TQACP632EQvyecJTG5wTvMuqy8a4f4TJVr".to_string(),
        spender: "0xA36B5Fec0E93d24908fAA9966535567E9f888994".to_string(),
        contract: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
        value: "2".to_string(),
        chain_code: "tron".to_string(),
        approve_type: "UN_LIMIT".to_string(),
    };

    let resp = wallet_manager.approve_fee(params).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_list() {
    let wallet_manager = get_manager().await;

    let uid = "f091ca89e48bc1cd3e4cb84e8d3e3d9e2564e3616efd1feb468793687037d66f".to_string();
    let account_id = 1;

    let resp = wallet_manager.approve_list(uid, account_id).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve_cancel() {
    let wallet_manager = get_manager().await;

    let params = ApproveReq {
        from: "TYskFdYh9zsx4XcVRtGY6KhdwgwinmEhSZ".to_string(),
        spender: "TMrVocuPpNqf3fpPSSWy7V8kyAers3p1Jc".to_string(),
        contract: "TXYZopYRdj2D9XRtbG411XZZ3kM5VkAeBf".to_string(),
        value: "15".to_string(),
        chain_code: "tron".to_string(),
        approve_type: "UN_LIMIT".to_string(),
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
        symbol: "eth".to_string(),
        decimals: 18,
    };

    let token_out = SwapTokenInfo {
        token_addr: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        symbol: "usdt".to_string(),
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
        recipient: "0xD5359AD68FDb8a72386aB26C68D7648D548ec70a".to_string(),
        chain_code: "eth".to_string(),
        amount_in: "0.1".to_string(),
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

    let o_value = "50";

    let amount_in = wallet_utils::unit::convert_to_u256(&o_value, 18).unwrap();

    let dex_route1 = DexRoute {
        percentage: "10000".to_string(),
        amount_in: amount_in.to_string(),
        amount_out: "0".to_string(),
        route_in_dex: vec![RouteInDex {
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
        }],
    };

    let token_in = SwapTokenInfo {
        token_addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        symbol: "USDT".to_string(),
        decimals: 18,
    };

    let token_out = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "WETH".to_string(),
        decimals: 18,
    };

    let req = SwapReq {
        aggregator_addr: AGGREGATOR.to_string(),
        amount_in: o_value.to_string(),
        amount_out: "0.01".to_string(),
        min_amount_out: "0.01".to_string(),
        chain_code: "eth".to_string(),
        recipient: "0xD5359AD68FDb8a72386aB26C68D7648D548ec70a".to_string(),
        token_in,
        token_out,
        dex_router: vec![dex_route1],
        allow_partial_fill: false,
    };

    let fee =
                r#"{"gasLimit":300000,"baseFee":"243155942","priorityFee":"1969633876","maxFeePerGas":"2800000000"}"#
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
