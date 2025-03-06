use wallet_transport_backend::request::SignedTranUpdateHashReq;

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
