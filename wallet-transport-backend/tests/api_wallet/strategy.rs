use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::swap::ApiInitSwapReq;

use crate::init;

#[serial_test::serial]
#[tokio::test]
async fn test_query_collect_strategy() -> Result<(), wallet_transport_backend::Error> {
    let sn = "b35f7b556b87c87bb1928ea6ab12ef6918b71f5c37fbd53b88e9353ea2093f0b";
    let backend_api = init(sn)?;
    let req =
        ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .query_collect_strategy("eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c")
        .await?;

    println!("[test_query_collect_strategy] res: {res:#?}");
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn test_query_withdrawal_strategy() -> Result<(), wallet_transport_backend::Error> {
    let sn = "b35f7b556b87c87bb1928ea6ab12ef6918b71f5c37fbd53b88e9353ea2093f0b";
    let backend_api = init(sn)?;
    let req =
        ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .query_withdrawal_strategy(
            "c91f78d83576dbaa8dce16285787aa2efbc9c0e606b54f7bc96e951d848496db",
        )
        .await?;

    println!("[test_query_withdrawal_strategy] res: {res:#?}");
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn test_query_api_wallet_configs() -> Result<(), wallet_transport_backend::Error> {
    let sn = "b35f7b556b87c87bb1928ea6ab12ef6918b71f5c37fbd53b88e9353ea2093f0b";
    let backend_api = init(sn)?;
    let req =
        ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let res = backend_api.query_api_wallet_configs().await?;
    let res = res.to_string();
    println!("[test_query_api_wallet_configs] res: {res:#?}");
    Ok(())
}
