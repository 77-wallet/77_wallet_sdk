use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::{
    request::api_wallet::{
        swap::ApiInitSwapReq,
        wallet::{AppIdImportReq, AppIdUidUsageReq, BindAppIdReq, InitApiWalletReq},
    },
    response_vo::api_wallet::wallet::UidStatus,
};

use crate::init;

#[tokio::test]
async fn test_query_wallet_activation_info() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .query_wallet_activation_info(
            "d3be315c670b5207190ea6fc88c9d8e4f71330b28433e54464f437dadd8c818e",
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
        .keys_uid_check("c91f78d83576dbaa8dce16285787aa2efbc9c0e606b54f7bc96e951d848496db")
        .await
        .unwrap();

    println!("[test_keys_uid_check] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_query_uid_bind_info() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let res = backend_api
        .query_uid_bind_info("c91f78d83576dbaa8dce16285787aa2efbc9c0e606b54f7bc96e951d848496db")
        .await
        .unwrap();

    println!("[test_query_uid_bind_info] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_init_api_wallet() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let mut req =
        InitApiWalletReq::new("5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d");

    req.set_recharge_uid("cf43155d5b80eb73beb6ce3c7224214f3ed33fcc2d4ebfe5764d36e1ffac8cce");
    let res = backend_api.init_api_wallet(req).await.unwrap();

    println!("[test_init_api_wallet] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_appid_uid_usage() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let req = AppIdUidUsageReq::new(
        "0583c23559bc44acb4132c59f0f2be21",
        "17931d2265113d34604598200350c0e5eba860af969768c91d5aee7f499c08c1",
        UidStatus::ApiWaw,
    );

    let res = backend_api.appid_uid_usage(req).await.unwrap();

    println!("[test_appid_uid_usage] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_wallet_bind_appid() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let req = BindAppIdReq::new(
        "88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640",
        "48c3ab3ce9d017ee8720e608646ff00a0957f7ea8f0d7edf38e868bcf06e6808",
        "bfd9b8aa4f384b839392e9018280e9fb",
        "3cf5ee2bf4971c12306cf24a1a2fabfac2a97e895f994325c935babc022185d3",
    );
    let res = backend_api.wallet_bind_appid(&req).await.unwrap();

    println!("[test_wallet_bind_appid] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_appid_import() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let mut req =
        AppIdImportReq::new("3cf5ee2bf4971c12306cf24a1a2fabfac2a97e895f994325c935babc022185d3");
    req.set_recharge_uid("87c2274b47f4b93329b9d686dae2c4bc0d96bdc4fd602320a4e87089bda7c915");
    req.set_withdrawal_uid("4080938dda41a016b8c153be34b558345259a4b4116d5a88e004507341164b78");

    let res = backend_api.appid_import(req).await.unwrap();

    println!("[test_appid_import] res: {res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_appid_withdrawal_wallet_change() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let withdrawal_uid = "17931d2265113d34604598200350c0e5eba860af969768c91d5aee7f499c08c1";
    let org_app_id = "0583c23559bc44acb4132c59f0f2be21";
    let res = backend_api.appid_withdrawal_wallet_change(withdrawal_uid, org_app_id).await.unwrap();

    println!("[test_appid_withdrawal_wallet_change] res: {res:#?}");
    Ok(())
}
