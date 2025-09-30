use wallet_transport_backend::request::api_wallet::wallet::InitApiWalletReq;

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

    let res = backend_api
        .keys_uid_check("1d4e2f6ca5dd1f1c25e2b7335bf8044574902ff82cea9943027e327e32505c27")
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
