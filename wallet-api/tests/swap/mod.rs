use crate::get_manager;
use wallet_api::request::transaction::ApproveParams;

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

#[tokio::test]
async fn test_approve() {
    let wallet_manager = get_manager().await;

    let params = ApproveParams {
        from: "0x1".to_string(),
        spender: "0x2".to_string(),
        contract: "0x3".to_string(),
        value: "100".to_string(),
        chain_code: "eth".to_string(),
    };

    let password = "123456".to_string();

    let resp = wallet_manager.approve(params, password).await.unwrap();
    println!("{}", serde_json::to_string(&resp).unwrap());
}
