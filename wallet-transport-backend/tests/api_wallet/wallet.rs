use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::{
    swap::ApiInitSwapReq, wallet::InitApiWalletReq,
};

use crate::init;

#[tokio::test]
async fn test_query_wallet_activation_info() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let res = backend_api
        .query_wallet_activation_info(
            "e6de8afd756e7cb81a3d965f959c896738ed07cebc919c7f96c97fc6069ad44f",
        )
        .await
        .unwrap();

    println!("[test_query_wallet_activation_info] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_keys_uid_check() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .keys_uid_check("0206aab9be69a5949ed958613806793290dffa74a177107c38070fbc526374fb")
        .await
        .unwrap();

    println!("[test_query_wallet_activation_info] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_init_api_wallet() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let mut req =
        InitApiWalletReq::new("5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d");

    req.set_recharge_uid("cf43155d5b80eb73beb6ce3c7224214f3ed33fcc2d4ebfe5764d36e1ffac8cce");
    let res = backend_api.init_api_wallet(req).await.unwrap();

    println!("[test_init_api_wallet] res: {res:#?}");
    Ok(())
}
