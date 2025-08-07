use crate::get_manager;

#[tokio::test]
async fn get_token_price() {
    let wallet_manager = get_manager().await;

    let symbol = vec!["WIN".to_string()];

    let detail = wallet_manager.get_token_price(symbol).await;

    println!(
        "{}",
        wallet_utils::serde_func::serde_to_string(&detail).unwrap(),
    );
}
