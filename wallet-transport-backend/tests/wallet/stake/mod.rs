use crate::init;

#[tokio::test]
async fn test_delegate_is_open() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?; // Initialize using init()

    let res = backend_api.delegate_is_open().await.unwrap();
    tracing::info!("{res:?}");

    Ok(())
}

#[tokio::test]
async fn test_delegate_complete() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?; // Initialize using init()

    let order = "672343049017657afff102f1";
    let res = backend_api.delegate_complete(&order).await.unwrap();
    tracing::info!("{res:?}");

    Ok(())
}

#[tokio::test]
async fn test_delegate_query_order() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?; // Initialize using init()

    let order = "66e6b46c3ebdf9433dcb3c49";
    let res = backend_api.delegate_query_order(&order).await.unwrap();
    tracing::info!("{res:?}");

    Ok(())
}

#[tokio::test]
async fn test_delegate_order() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?; // Initialize using init()

    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let energy = 10000;
    let res = backend_api.delegate_order(&address, energy).await.unwrap();
    tracing::info!("{res:?}");

    Ok(())
}

#[tokio::test]
async fn test_vote_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?; // Initialize using init()

    let res = backend_api.vote_list().await.unwrap();
    tracing::info!("{res:#?}");

    Ok(())
}
