use crate::get_manager;
use wallet_api::{
    domain::swap_client::{DexRoute, RouteInDex},
    request::transaction::{ApproveParams, DepositParams, QuoteReq, SwapReq},
};

#[tokio::test]
async fn test_token_list() {
    let wallet_manager = get_manager().await;

    let resp = wallet_manager.token_list().await;
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

    let params = ApproveParams {
        from: "0x14AdbbE60b214ebddc90792482F664C446d93804".to_string(),
        spender: "0x59a4ad52B1dEfC42033f8f109a7cF53924296112".to_string(),
        contract: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        value: "10".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let a = Box::new(1);


    let resp = wallet_manager.approve(params, password).await.unwrap();
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_deposit() {
    let wallet_manager = get_manager().await;

    let params = DepositParams {
        from: "0x14AdbbE60b214ebddc90792482F664C446d93804".to_string(),
        contract: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        value: "10".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.deposit(params, password).await.unwrap();
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_deposit_and_approve() {

    let wallet_manager = get_manager().await;

    // deposit 
    let params = DepositParams {
        from: "0x14AdbbE60b214ebddc90792482F664C446d93804".to_string(),
        contract: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        value: "10".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.deposit(params, password.clone()).await.unwrap();
    println!("deposit = {}", serde_json::to_string(&resp).unwrap());


    let params = ApproveParams {
        from: "0x14AdbbE60b214ebddc90792482F664C446d93804".to_string(),
        spender: "0x59a4ad52B1dEfC42033f8f109a7cF53924296112".to_string(),
        contract: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        value: "10".to_string(),
        chain_code: "eth".to_string(),
    };

    let resp = wallet_manager.approve(params, password).await.unwrap();
    println!("{}", serde_json::to_string(&resp).unwrap());
}

// (选择的流动性)流动性
#[tokio::test]
async fn test_quote() {
    let wallet_manager = get_manager().await;

    let req = QuoteReq {
        from: "".to_string(),
        chain_code: "eth".to_string(),
        amount_in: "0.2".to_string(),
        token_in: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        token_out: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        dex_list: vec![2, 3],
        from_symbol: "eth".to_string(),
        slippage: 0.2,
    };

    let result = wallet_manager.quote(req).await;
    tracing::warn!("quote = {}", serde_json::to_string(&result).unwrap());
}

#[tokio::test]
async fn test_swap() {
    let wallet_manager = get_manager().await;

    let dex_route1 = DexRoute {
        amount_in: "10000000000000000".to_string(),
        amount_out: "0".to_string(),
        route_in_dex: vec![
            RouteInDex {
                dex_id: 3,
                pool_id: "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640".to_string(),
                in_token_addr: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
                out_token_addr: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                zero_for_one: false,
                fee: "500".to_string(),
                amount_in: 0.to_string(),
                min_amount_out: 0.to_string(),
            },
            RouteInDex {
                dex_id: 2,
                pool_id: "0x3041CbD36888bECc7bbCBc0045E3B1f144466f5f".to_string(),
                in_token_addr: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
                out_token_addr: "0xdac17f958d2ee523a2206206994597c13d831ec7".to_string(),
                zero_for_one: true,
                fee: "3000".to_string(),
                amount_in: 0.to_string(),
                min_amount_out: 0.to_string(),
            },
        ],
    };

    let req = SwapReq {
        chain_code: "eth".to_string(),
        recipient: "0x14AdbbE60b214ebddc90792482F664C446d93804".to_string(),
        token_in: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2".to_string(),
        token_out: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
        dex_router: vec![dex_route1],
        allow_partial_fill: false,
    };

  let fee = 
            r#"{"gasLimit":300000,"baseFee":"0","priorityFee":"1000000000","maxFeePerGas":"1000000000"}"#
                .to_string();
    let password = "123456".to_string();

    let result = wallet_manager.swap(req, fee, password).await;
    tracing::warn!("swap hash = {}", serde_json::to_string(&result).unwrap());
}
