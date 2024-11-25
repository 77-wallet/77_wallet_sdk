use std::collections::HashMap;
use wallet_chain_interact::{eth::EthChain, eth::Provider};
use wallet_transport::client::RpcClient;
use wallet_utils::init_test_log;
pub mod multisig;

mod base;
fn get_chain() -> EthChain {
    init_test_log();
    // alchemy eth sep 测试网络
    let rpc = "https://eth-sepolia.g.alchemy.com/v2/I6EHAmjDJfTGik1rvtt6TRTGwARrBFtg";
    // let rpc = "https://eth-mainnet.g.alchemy.com/v2/I6EHAmjDJfTGik1rvtt6TRTGwARrBFtg";
    // let rpc =
    //     "https://rpc.ankr.com/eth/bfc95e452dfb2153a5645698ee67d4d543e760ebcf045d14a104a52356236f6d";

    let header = Some(HashMap::from([(
        "client_id".to_string(),
        "xxxx".to_string(),
    )]));

    let client = RpcClient::new(&rpc, header).unwrap();
    let provider = Provider::new(client).unwrap();
    let network = wallet_types::chain::network::NetworkKind::Testnet;
    let eth = EthChain::new(provider, network).unwrap();
    eth
}
