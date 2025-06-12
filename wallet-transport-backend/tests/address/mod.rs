use crate::init;
use wallet_transport_backend::request::{AddressDetailsReq, AddressInitReq};

#[tokio::test]
async fn test_address_init() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let uid = "cd2ac48fa33ba24a8bc0d89e7658a2cd";
    let req = AddressInitReq {
        uid: uid.to_string(),
        address: "TLzteCJi4jSGor5EDRYZcdQ4hsZRQQZ4XR".to_string(),
        index: 0,
        chain_code: "tron".to_string(),
        sn: "3f76bd432e027aa97d11f2c3f5092bee195991be461486f0466eec9d46940e9e".to_string(),
        contract_address: vec!["".to_string()],
        name: "test".to_string(),
    };

    let res = backend_api.address_init(&req).await.unwrap();

    println!("[test_chain_default_list] res: {res:?}");
    Ok(())
}

#[tokio::test]
async fn test_address_details() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let req = AddressDetailsReq {
        // address: "TSL4wp6qcLwub88FmEu2gozA1Buz8CnsTn".to_string(),
        // address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
        // address: "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N".to_string(),
        address: "TAU1t14o8zZksWRKjwqAVPTMXczUZzvMR1".to_string(),
        chain_code: "tron".to_string(),
    };

    let res = backend_api
        .address_find_multisiged_details(req)
        .await
        .unwrap();

    println!("[test_chain_default_list] res: {res:?}");
    Ok(())
}

#[tokio::test]
async fn test_assests_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let uid = "074209f318e1079c7910c336df5745c57d31da251ebecd7cfda6d13206b71699".to_string();
    let address = None;
    let res = backend_api.wallet_assets_list(uid, address).await;

    println!(" res: {res:?}");
    Ok(())
}
