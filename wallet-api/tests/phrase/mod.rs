use crate::get_manager;

#[tokio::test]
async fn test_phrase() {
    let wallet_manager = get_manager().await;

    let phrase = wallet_manager.generate_phrase(1, 12);

    //
    tracing::info!("{phrase:?}")
}
