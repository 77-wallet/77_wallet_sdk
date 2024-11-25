use wallet_chain_interact::tron::{Provider, TronBlockChain, TronChain, TronProvider};
use wallet_transport::client::HttpClient;
use wallet_utils::init_test_log;

mod account;
mod base;
mod multisig;
mod stake;

fn get_chain() -> TronChain {
    init_test_log();
    // let url = "https://api.nileex.io";
    let url = "https://api.trongrid.io";
    // let url = "https://rpc.ankr.com/premium-http/tron/32c99b7a1638ea489c32e97b41a22deb7df585d751f194eb4cc57e49649892eb";

    let http_client = HttpClient::new(&url, None).unwrap();
    let provider = Provider::new(http_client).unwrap();

    TronChain::new(provider).unwrap()
}
fn get_chain_stake() -> TronBlockChain {
    init_test_log();
    // let url = "https://api.nileex.io";
    let url = "https://api.trongrid.io";
    let provider = TronProvider::new(url).unwrap();
    TronBlockChain::new(provider).unwrap()
}
