use wallet_transport_backend::request::FindConfigByKey;

use crate::init;

#[tokio::test]
async fn test_find_config_by_key() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = FindConfigByKey {
        key: "OFFICIAL:WEBSITE".to_string(),
    };

    let res = backend_api
        .find_config_by_key(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[find_config_by_key] res: {res:?}");

    Ok(())
}
