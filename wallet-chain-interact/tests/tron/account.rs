use crate::tron::get_chain_stake;

#[tokio::test]
async fn test_account_resource() {
    let chain = get_chain_stake();
    let account = "TQFgfqwWS158CSHUmJ9gPzzGaR78fTspcQ";
    // let account = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let res = chain.account_resource(&account).await.unwrap();
    tracing::info!("{:?}", res);
}

#[tokio::test]
async fn test_parameter() {
    let chain = get_chain_stake();
    let res = chain.chain_parameter().await.unwrap();
    tracing::info!("{:?}", res);
}
