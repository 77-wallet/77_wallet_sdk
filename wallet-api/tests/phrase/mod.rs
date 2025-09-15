use crate::get_manager;
use anyhow::Result;

#[tokio::test]
async fn test_phrase() -> Result<()> {
    let wallet_manager = get_manager().await;

    let res = wallet_manager.generate_phrase(1, 12)?;
    let output = res.phrases.join(" ");
    tracing::info!("{}", output);

    Ok(())
}
