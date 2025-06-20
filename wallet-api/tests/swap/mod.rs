use crate::get_manager;

// #[tokio::test]
// async fn test_quote() {
//     let wallet_manager = get_manager().await;
// }

#[tokio::test]
async fn test_token_list() {
    let wallet_manager = get_manager().await;

    let resp = wallet_manager.token_list().await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}

#[tokio::test]
async fn test_support_chain() {
    let wallet_manager = get_manager().await;

    let resp = wallet_manager.chain_list().await;
    println!("{}", serde_json::to_string(&resp).unwrap());
}
