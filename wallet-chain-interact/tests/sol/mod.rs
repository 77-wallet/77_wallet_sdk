mod address;
mod base;
mod multisig;
use std::collections::HashMap;

use wallet_chain_interact::sol::{Provider, SolanaChain};
use wallet_transport::client::RpcClient;
use wallet_utils::init_test_log;

fn get_chain() -> SolanaChain {
    init_test_log();
    // let rpc = "https://api.testnet.solana.com";
    let rpc = "https://api.devnet.solana.com";
    // let rpc = "https://api.mainnet-beta.solana.com";
    // let rpc = "https://rpc.ankr.com/solana/32c99b7a1638ea489c32e97b41a22deb7df585d751f194eb4cc57e49649892eb";
    // let rpc = "https://rpc.ankr.com/solana/1de0b2109f36eb77145fdd195bcf3fbad10cc878d49e1ade16f550f1b4115c26";

    let header = Some(HashMap::from([(
        "client_id".to_string(),
        "xxxx".to_string(),
    )]));

    let client = RpcClient::new(&rpc, header).unwrap();
    let provider = Provider::new(client).unwrap();

    SolanaChain::new(provider).unwrap()
}
