use crate::init;
use wallet_transport_backend::{
    request::{
        CustomTokenInitReq, TokenCancelSubscribeReq, TokenQueryByContractAddressReq,
        TokenQueryByPageReq, TokenQueryHistoryPrice, TokenQueryPopularByPageReq, TokenQueryPrice,
        TokenQueryPriceReq, TokenSubscribeReq,
    },
    Error,
};

#[tokio::test]
async fn test_custom_token_init() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = CustomTokenInitReq {
        chain_code: "eth".to_string(),
        symbol: "XRP".to_string(),
        token_name: "XRP(IBC)".to_string(),
        contract_address: Some("0x628F76eAB0C1298F7a24d337bBbF1ef8A1Ea6A24".to_string()),
        master: false,
        unit: 6,
    };
    let res = backend_api.custom_token_init(req).await.unwrap();

    println!("[test_custom_token_init] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_subscribe() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenSubscribeReq {
        chain_code: "eth".to_string(),
        address: "1".to_string(),
        index: Some(0),
        contract_account_address: None,
        uid: "1".to_string(),
        sn: "1".to_string(),
        app_id: "1".to_string(),
        device_type: Some("ANDROID".to_string()),
    };
    let res = backend_api.token_subscribe(req).await.unwrap();

    println!("[test_token_subscribe] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_query_currency() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let res = backend_api
        .token_query_by_currency("tron", "USDT", "trx")
        .await
        .unwrap();

    tracing::info!("[test_token_query_price] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_cancel_subscribe() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenCancelSubscribeReq {
        address: "".to_string(),
        contract_address: "".to_string(),
        sn: "".to_string(),
    };
    let res = backend_api.token_cancel_subscribe(req).await.unwrap();

    println!("[test_token_cancel_subscribe] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_query_price() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryPriceReq(vec![TokenQueryPrice {
        chain_code: "ltc".to_string(),
        contract_address_list: vec!["".to_string()],
    }]);
    let res = backend_api.token_query_price(req).await.unwrap();

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("[test_token_query_price] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn test_default_token_list() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryByPageReq::new_default_token(Vec::new(), 0, 100);

    let res = backend_api.token_query_by_page(&req).await.unwrap();

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    println!("[test_token_list] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn test_popular_token_list() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryByPageReq::new_popular_token(0, 1000);

    let res = backend_api.token_query_by_page(&req).await.unwrap();

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    println!("[test_popular_token_list] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn test_token_list() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryByPageReq::new_token(0, 1000);

    let res = backend_api.token_query_by_page(&req).await.unwrap();
    println!("[test_token_list] res: {res:?}");
    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    println!("[test_popular_token_list] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn test_token_query_by_contract_address() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryByContractAddressReq {
        chain_code: "tron".to_string(),
        contract_address: "".to_string(),
    };

    let res = backend_api
        .token_query_by_contract_address(&req)
        .await
        .unwrap();

    println!("[test_token_query_by_contract_address] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_query_his_price() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryHistoryPrice {
        chain_code: "tron".to_string(),
        date_type: "DAY".to_string(),
        currency: "USD".to_string(),
        contract_address: "TSSMHYeV2uE9qYH95DqyoCuNCzEL1NvU3S".to_string(),
    };

    let res = backend_api.query_history_price(&req).await.unwrap();

    println!("[test_token_query_his_price] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_token_rates() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let res = backend_api.token_rates().await.unwrap();

    let res_str = serde_json::to_string(&res).unwrap();
    tracing::info!("[test_token_rates] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn test_query_popular_by_page() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryPopularByPageReq::new(
        None,
        None,
        Some("marketValue".to_string()),
        Some("DESC".to_string()),
        0,
        300,
    );

    let res = backend_api.query_popular_by_page(&req).await.unwrap();

    let res_str = serde_json::to_string(&res).unwrap();
    tracing::info!("[test_query_popular_by_page] res: {res_str}");

    Ok(())
}

#[tokio::test]
async fn _test_token_query_price() -> Result<(), Error> {
    let backend_api = init()?; // 初始化加密器和API

    let req = TokenQueryPriceReq(vec![TokenQueryPrice {
        chain_code: "eth".to_string(),
        contract_address_list: vec!["0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84".to_string()],
    }]);
    let res = backend_api._token_query_price(req).await.unwrap();

    println!("[_test_token_query_price] res: {res:?}");

    let res_str = serde_json::to_string(&res).unwrap();
    tracing::info!("[_test_token_query_price] res: {res_str}");

    Ok(())
}
