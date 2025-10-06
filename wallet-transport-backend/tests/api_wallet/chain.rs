use crate::init;

#[tokio::test]
async fn test_api_wallet_chain_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let res = backend_api.api_wallet_chain_list("2.0.0").await.unwrap();

    // println!("[test_chain_default_list] res: {res:?}");
    let res = serde_json::to_string(&res).unwrap();
    println!("[test_api_wallet_chain_list] res: {res:?}");
    Ok(())
}
