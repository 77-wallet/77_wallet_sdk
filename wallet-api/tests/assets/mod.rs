use crate::get_manager;

#[tokio::test]
async fn test_asset_detail() {
    let wallet = get_manager().await;

    let address = "0xca93df9d481ff298080047e612dac1ff537d3e24a843e2608428848a108083ec";
    let account_id = None;
    let chain_code = "sui";
    let symbol = "USDT";
    let token_address = None;
    let assets = wallet
        .get_assets(address, account_id, chain_code, symbol, token_address)
        .await;

    tracing::warn!("assets: {:?}", serde_json::to_string(&assets).unwrap());
}

#[tokio::test]
async fn sync_assets() {
    let wallet_manager = get_manager().await;
    let addr = "0xEB2b4F967D9a6BeA958dDe3e5814BbE33A5CBfE2".to_string();
    let chain_code = None;
    let symbol = vec![];

    let _c = wallet_manager.sync_assets(addr, chain_code, symbol).await;
    tracing::info!("response");
}

#[tokio::test]
async fn sync_assets_from_chain() {
    let wallet_manager = get_manager().await;
    let addr = "0xEB2b4F967D9a6BeA958dDe3e5814BbE33A5CBfE2".to_string();
    let chain_code = None;
    let symbol = vec![];

    let _c = wallet_manager
        .sync_balance_from_chain(addr, chain_code, symbol)
        .await;
    tracing::info!("response");
}

#[tokio::test]
async fn sync_assets_by_wallet() {
    let wallet_manager = get_manager().await;
    let wallet_address = "0xEB2b4F967D9a6BeA958dDe3e5814BbE33A5CBfE2".to_string();
    let account_id = Some(1);
    let symbol = vec![];

    let _c = wallet_manager
        .sync_assets_by_wallet(wallet_address, account_id, symbol)
        .await;
}
