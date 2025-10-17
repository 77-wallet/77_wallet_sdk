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

    let res = backend_api
        .query_withdrawal_strategy(
            "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c",
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
