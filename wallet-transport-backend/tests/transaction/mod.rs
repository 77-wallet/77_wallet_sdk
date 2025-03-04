use wallet_transport_backend::request::{SignedTranAcceptReq, SignedTranUpdateHashReq};

use crate::init;

#[tokio::test]
pub async fn test_fee_oracle() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize using init()

    let res = backend_api
        .gas_oracle(&aes_cbc_cryptor, "eth")
        .await
        .unwrap();
    tracing::info!("res  {:?}", res);

    Ok(())
}

#[tokio::test]
pub async fn test_signed_tran_accept() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize using init()

    let address = vec![
        "TBA5hXR9mm6kpzFMwnh3dkqham4d9GQH8w".to_string(),
        "TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6".to_string(),
        "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ".to_string(),
    ];
    let str = r#"[{\"queue_id\":\"155061461155188736\",\"address\":\"TBA5hXR9mm6kpzFMwnh3dkqham4d9GQH8w\",\"signature\":\"be03fac326d23804cb97074743cd7b9a96bf5b3bebeb586bc100cc8d3e77dc29129ee748fa909131a84178c87e46abf84b5f5b8ba9c4492399c1bc7c406b9cd201\",\"status\":1},{\"queue_id\":\"155061461155188736\",\"address\":\"TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6\",\"signature\":\"a26013db79e4ee9f24a84aa27d8ad3ccc68405b57baf8b8ed90e0829c3d3320e6adc3ea82a0867f05fdb0f2ab745aa3f40a767f806f328af8418ee137e376b5a01\",\"status\":1},{\"queue_id\":\"155061461155188736\",\"address\":\"TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ\",\"signature\":\"f9563beea8a050e8dac7917eb2e0ea002e8f819ae03a9e8fcefe887e5bbe872f2a5e6cc49783d635c9468fd0ef929355af012f8de6d02483fbffb35cedde03c901\",\"status\":1}]"#;
    let req = SignedTranAcceptReq {
        withdraw_id: "155061461155188736".to_string(),
        accept_address: address,
        tx_str: serde_json::json!(str),
        status: 1,
        raw_data: "".to_string(),
    };

    let res = backend_api
        .signed_tran_accept(&aes_cbc_cryptor, &req)
        .await
        .unwrap()
        .unwrap();
    tracing::info!("res  {:?}", res);

    Ok(())
}

#[tokio::test]
pub async fn test_signed_tran_update_hash() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize using init()

    let req = SignedTranUpdateHashReq {
        withdraw_id: "155061461155188736".to_string(),
        hash: "xxxxxxxxxxxx".to_string(),
        remark: "hello".to_string(),
        raw_data: "".to_string(),
    };

    let res = backend_api
        .signed_tran_update_trans_hash(&aes_cbc_cryptor, &req)
        .await
        .unwrap();
    tracing::info!("res  {:?}", res);

    Ok(())
}

#[tokio::test]
pub async fn test_records_list() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize using init()

    let cc = backend_api
        .record_lists(
            &aes_cbc_cryptor,
            "tron",
            "TDmNZ4Wz7aMEt1tbRq7EVocWkxWn2SLoPE",
            Some("2024-09-28 00:00:00".to_string()),
        )
        .await
        .unwrap();

    println!("[record] res: {cc:#?}");

    Ok(())
}
