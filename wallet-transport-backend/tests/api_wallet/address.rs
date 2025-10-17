use crate::init;
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::{
    AddressInitReq,
    api_wallet::{
        address::{AddressListReq, ApiAddressInitReq, AssetListReq},
        swap::ApiInitSwapReq,
    },
};

#[tokio::test]
async fn test_expand_address() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";
    let mut req = ApiAddressInitReq::new();
    let address_param = AddressInitReq::new(
        uid,
        "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt",
        1,
        "tron",
        "1",
        vec![],
        "test",
    );
    req.address_list.add_address(address_param);

    let res = backend_api.expand_address(&req).await.unwrap();

    println!("[test_chain_default_list] res: {res:?}");
    Ok(())
}

#[tokio::test]
async fn test_expand_address_complete() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";

    backend_api.expand_address_complete(uid, "1").await.unwrap();

    Ok(())
}

#[tokio::test]
async fn test_query_used_address_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let uid = "2b607a707cc4f0b4191bce26149e0310302905a59aed4c27b35d6429bfacd5d9";
    let chain_code = "tron";
    let page_num = 1;
    let page_size = 1;
    let req = AddressListReq::new(uid, chain_code, page_num, page_size);
    let res = backend_api.query_used_address_list(&req).await.unwrap();
    let res = serde_json::to_string(&res).unwrap();
    println!("{res:#?}");
    Ok(())
}

#[tokio::test]
async fn test_query_asset_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req =
        ApiInitSwapReq { sn: "wenjing".to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let uid = "2b607a707cc4f0b4191bce26149e0310302905a59aed4c27b35d6429bfacd5d9";
    // let uid = "2b3c9d25a6d68fd127a77c4d8fefcb6c2466ac40e5605076ee3e1146f5f66993";
    let chain_code = "tron";
    let req = AssetListReq::new(uid, chain_code, vec![0, 1]);
    let res = backend_api.query_asset_list(&req).await.unwrap();
    let res = serde_json::to_string(&res).unwrap();
    println!("{res:#?}");
    Ok(())
}
