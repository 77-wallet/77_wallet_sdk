use crate::init;

#[tokio::test]
async fn test_query_wallet_activation_info() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let res = backend_api
        .query_wallet_activation_info(
            "0xF1C1FE41b1c50188faFDce5f21638e1701506f1b",
        )
        .await
        .unwrap();

    println!("[test_query_wallet_activation_info] res: {res:#?}");
    Ok(())
}
