use crate::init;

#[serial_test::serial]
#[tokio::test]
async fn test_swap() -> Result<(), wallet_transport_backend::Error> {
    let sn = "666";
    let backend_api = init(sn)?;

    let res = backend_api
        .query_collect_strategy("eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c")
        .await?;

    println!("[test_query_collect_strategy] res: {res:#?}");
    Ok(())
}
