use crate::get_manager;
use wallet_api::request::transaction::{
    ApproveReq, DexRoute, QuoteReq, RouteInDex, SwapReq, SwapTokenInfo, SwapTokenListReq,
};

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
async fn test_token_list() {
    let wallet_manager = get_manager().await;

    let req = SwapTokenListReq {
        chain_code: "tron".to_string(),
        search: "".to_string(),
        wallet_address: "0xab17D82c9810c91DAd8842D4C8f1771DB39919bC".to_string(),
        account_id: 1,
        exclude_token: vec![],
        page_num: 0,
        page_size: 3,
    };

    let resp = wallet_manager.token_list(req).await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_approve() {
    let wallet_manager = get_manager().await;
    // TMrVocuPpNqf3fpPSSWy7V8kyAers3p1Jc
    let params = ApproveReq {
        from: "TQACP632EQvyecJTG5wTvMuqy8a4f4TJVr".to_string(),
        spender: "TTXoDkNZWeTCFqeRs8G5QPjewZduUstPcT".to_string(),
        contract: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
        value: "2".to_string(),
        chain_code: "tron".to_string(),
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
        spender: "TTXoDkNZWeTCFqeRs8G5QPjewZduUstPcT".to_string(),
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

    // TNUC9Qb1rRpS5CbWLmNMxXBjyFoydXjWFR wtrx
    // TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t usdt
    // TSSMHYeV2uE9qYH95DqyoCuNCzEL1NvU3S sun

    let token_in = SwapTokenInfo {
        token_addr: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
        symbol: "TRX".to_string(),
        decimals: 6,
    };

    let token_out = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "TRX".to_string(),
        decimals: 6,
    };

    let req = QuoteReq {
        aggregator_addr: "TTXoDkNZWeTCFqeRs8G5QPjewZduUstPcT".to_string(),
        recipient: "TQACP632EQvyecJTG5wTvMuqy8a4f4TJVr".to_string(),
        chain_code: "tron".to_string(),
        amount_in: "2".to_string(),
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

    let o_value = "2";

    let amount_in = wallet_utils::unit::convert_to_u256(&o_value, 6).unwrap();

    let dex_route1 = DexRoute {
        percentage: "10000".to_string(),
        amount_in: amount_in.to_string(),
        amount_out: "0".to_string(),
        route_in_dex: vec![
            RouteInDex {
                dex_id: 3,
                pool_id: "TSUUVjysXV8YqHytSNjfkNXnnB49QDvZpx".to_string(),
                in_token_symbol: "TUSD".to_string(),
                in_token_addr: "TNUC9Qb1rRpS5CbWLmNMxXBjyFoydXjWFR".to_string(),
                out_token_addr: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
                out_token_symbol: "USDT".to_string(),
                zero_for_one: true,
                fee: "500".to_string(),
                amount_in: 0.to_string(),
                min_amount_out: 0.to_string(),
            },
            // RouteInDex {
            //     dex_id: 2,
            //     pool_id: "TTdeCobmYxhfFBYUZbiQqbZ56zrFkSE5DG".to_string(),
            //     in_token_symbol: "USDT".to_string(),
            //     in_token_addr: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
            //     out_token_addr: "TSSMHYeV2uE9qYH95DqyoCuNCzEL1NvU3S".to_string(),
            //     out_token_symbol: "SUN".to_string(),
            //     zero_for_one: true,
            //     fee: "3000".to_string(),
            //     amount_in: 0.to_string(),
            //     min_amount_out: 0.to_string(),
            // },
        ],
    };

    let token_in = SwapTokenInfo {
        token_addr: "".to_string(),
        symbol: "WTRX".to_string(),
        decimals: 6,
    };

    let token_out = SwapTokenInfo {
        token_addr: "AADd".to_string(),
        symbol: "Hello Coin".to_string(),
        decimals: 7,
    };

    let req = SwapReq {
        aggregator_addr: "TTXoDkNZWeTCFqeRs8G5QPjewZduUstPcT".to_string(),
        amount_in: o_value.to_string(),
        amount_out: "1".to_string(),
        min_amount_out: "0".to_string(),
        chain_code: "tron".to_string(),
        recipient: "TQACP632EQvyecJTG5wTvMuqy8a4f4TJVr".to_string(),
        token_in,
        token_out,
        dex_router: vec![dex_route1],
        allow_partial_fill: false,
    };

    let fee =
                r#"{"gasLimit":3000000,"baseFee":"0","priorityFee":"1000000000","maxFeePerGas":"1000000000"}"#
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
