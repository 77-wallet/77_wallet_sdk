use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::swap::ApiInitSwapReq;

use crate::init;

#[tokio::test]
async fn test_query_collect_strategy() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let res = backend_api
        .query_collect_strategy("eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c")
        .await
        .unwrap();

    println!("[test_query_collect_strategy] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_query_withdrawal_strategy() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .query_withdrawal_strategy(
            "703dc9ffe712d3ced169cee62c3c9c8118ce822bd00d49650e02df80ba0fcc30",
        )
        .await
        .unwrap();

    println!("[test_query_withdrawal_strategy] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_query_api_wallet_configs() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let res = backend_api.query_api_wallet_configs().await.unwrap();
    let res = res.to_string();
    println!("[test_query_api_wallet_configs] res: {res:#?}");
    Ok(())
}
