use crate::bsc::get_chain;
use wallet_chain_interact::{
    eth::{operations, FeeSetting},
    types::ChainPrivateKey,
};
use wallet_utils::unit;

#[tokio::test]
async fn test_get_balance() {
    let instance = get_chain();

    let addr = "0x246FA6bcFaaDdc4C968478D89075Aec77fC1873E";
    // let token = Some("0x779877A7B0D9E8603169DdbD7836e478b4624789".to_string());
    let token: Option<String> = None;
    let rs = instance.balance(addr, token).await;

    tracing::info!("balance={rs:?}");
    assert!(rs.is_ok())
}

#[tokio::test]
async fn test_estimate_gas() {
    let instance = get_chain();

    let from = "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1";
    let to = "0x5985CE40d3dACf7c1352e464691BC7fb03215928";
    let value = unit::convert_to_u256("0.01", 18).unwrap();

    let base = operations::EthereumBaseTransaction::new(from, to, value).unwrap();
    let prams = operations::TransferOpt {
        base,
        contract: None,
    };
    let fee = instance.estimate_gas(prams).await.unwrap();

    tracing::info!("fee={fee:?}");
}

#[tokio::test]
async fn test_transfer() {
    let instance = get_chain();

    let from = "0x3EC161C02Cd5a49EE8657947DC99DA58D1259aA1";
    let to = "0xdc4778f200c36a1C9dEeb3164cEE8366aD1F9455";
    let value = unit::convert_to_u256("0.3", 18).unwrap();

    let params = operations::TransferOpt::new(from, to, value, None).unwrap();

    let fee = FeeSetting::default();
    let key =
        ChainPrivateKey::from("0x34513a4c3547035a6b6ece66ab2cf41da990e15f607c379f71d079a4b4862862");

    let fee = instance.exec_transaction(params, fee, key).await.unwrap();

    tracing::info!("tx_hash={fee:?}");
}

#[tokio::test]
async fn test_query_tx_rs() {
    let instance = get_chain();
    let tx = "0xee344bba865f0e8bd764a14772554b0e87b3f0a104cec2b68ca22f5a894f3227";
    let res = instance.query_tx_res(tx).await;

    tracing::info!("{res:?}");
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_black_address() {
    let chain = get_chain();

    let token = "0x55d398326f99059fF775485246999027B3197955";
    let owner = "0xF4FC38Ae6E6B369a80F086eFD95dBF60D41E7fA5";

    let res = chain.black_address(token, owner).await.unwrap();

    tracing::info!("{res:?}");
}
