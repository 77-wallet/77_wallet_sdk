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
