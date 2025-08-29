use std::collections::HashMap;

use crate::get_manager;

#[tokio::test]
async fn get_token_price() {
    let wallet_manager = get_manager().await;

    let symbol = vec!["WIN".to_string()];

    let detail = wallet_manager.get_token_price(symbol).await;

    println!("{}", wallet_utils::serde_func::serde_to_string(&detail).unwrap(),);
}

#[tokio::test]
async fn token_market_value() {
    let wallet_manager = get_manager().await;

    let coins = HashMap::from([("eth".to_string(), "".to_string())]);

    let detail = wallet_manager.coin_market_value(coins).await;

    println!("{}", wallet_utils::serde_func::serde_to_string(&detail).unwrap(),);
}
