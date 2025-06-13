use crate::get_manager;

#[tokio::test]
async fn test_chain_list() {
    let wallet = get_manager().await;

    let wallet_address = "0xEB2b4F967D9a6BeA958dDe3e5814BbE33A5CBfE2";
    let account = 1;
    let symbol = "USDT";

    let assets = wallet
        .get_chain_list(&wallet_address, account, symbol)
        .await;

    tracing::warn!("chain_list: {:#?}", assets);
}
