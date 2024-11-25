use crate::eth::get_chain;
use wallet_chain_interact::{
    eth::{operations, FeeSetting},
    types::ChainPrivateKey,
};
use wallet_utils::unit;

#[tokio::test]
async fn test_get_balance() {
    let instance = get_chain();

    let addr = "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1";
    // let token = Some("0xdac17f958d2ee523a2206206994597c13d831ec7".to_string());
    let token: Option<String> = None;
    let rs = instance.balance(addr, token).await;

    let balance = rs.unwrap();
    let balance = wallet_utils::unit::format_to_string(balance, 18).unwrap();

    tracing::info!("balance={balance:?}");
    // assert!(rs.is_ok())
}

#[tokio::test]
async fn test_estimate_gas() {
    let instance = get_chain();

    let from = "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1";
    let to = "0x5985CE40d3dACf7c1352e464691BC7fb03215928";
    let value = unit::convert_to_u256("0.01", 18).unwrap();

    let params = operations::TransferOpt::new(from, to, value, None).unwrap();

    let fee = instance.estimate_gas(params).await.unwrap();
    tracing::info!("fee={fee:?}");
}

#[tokio::test]
async fn test_transfer() {
    let instance = get_chain();

    let from = "0x3f17f1962B36e491b30A40b2405849e597Ba5FB5";
    let to = "0x5ce57b7af50f9a9f66e5f157617ebeb183f532f4";
    let value = unit::convert_to_u256("9", 18).unwrap();

    let params = operations::TransferOpt::new(from, to, value, None).unwrap();

    let fee = FeeSetting::default();
    let key = ChainPrivateKey::from("1");

    let fee = instance.exec_transaction(params, fee, key).await.unwrap();

    tracing::info!("fee={fee:?}");
}

#[tokio::test]
async fn test_query_tx_rs() {
    let instance = get_chain();
    let tx = "0xe10d96ec2a982bd062abe40d347ef4e7b92a6ebc341afbba0b7c13f79241b746";
    let res = instance.query_tx_res(tx).await;

    tracing::info!("{res:?}");
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_chain_id() {
    let instance = get_chain();
    let res = instance.provider.chain_id().await.unwrap();
    tracing::info!("{res:?}");
}

#[tokio::test]
async fn test_default_fee() {
    let instance = get_chain();
    let res = instance.provider.get_default_fee().await.unwrap();
    tracing::info!("{res:?}");
}

#[tokio::test]
async fn test_black_address() {
    let chain = get_chain();

    let token = "0xdac17f958d2ee523a2206206994597c13d831ec7";
    let owner = "0xda57F5e0702cB772f9Ff25f370B2ec994c757823";

    let res = chain.black_address(token, owner).await.unwrap();

    tracing::info!("{res:?}");
}

#[tokio::test]
async fn test_get_block() {
    let chain = get_chain();

    let hash = 20998540;
    let res = chain.provider.get_block(hash).await.unwrap();

    let txs = res.transactions.as_transactions().unwrap();

    tracing::warn!("交易数量{:?}", txs.len());

    let tx_hash = "0x3be4057d55441b1804c54dcc84c409b3751fbe476bfaf1f294c451cd727683ac";

    let result = txs.iter().find(|t| t.hash.to_string().as_str() == tx_hash);

    tracing::warn!("{:#?}", result)
}

#[tokio::test]
async fn test_get_transaction() {
    let chain = get_chain();

    let hash = "0x8f773e9551d274d97d33b4009ffeeaef3e64396d6ffe8e41eb865909ce7a1381";
    let res = chain.provider.transaction_receipt(hash).await.unwrap();

    tracing::warn!("{:#?}", res)
}
