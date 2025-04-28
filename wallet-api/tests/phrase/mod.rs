use crate::get_manager;

#[tokio::test]
async fn test_phrase() {
    let wallet_manager = get_manager().await;

    let phrase = wallet_manager.generate_phrase(1, 12);

    if let Some(res) = phrase.result {
        let output = res.phrases.join(" ");
        tracing::info!("{}", output);
    }
}
