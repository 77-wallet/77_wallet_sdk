use wallet_transport_backend::{
    Error,
    request::{ChainListReq, ChainRpcListReq},
};

use crate::init;

#[tokio::test]
async fn test_chain_default_list() -> Result<(), Error> {
    let backend_api = init()?; // Initialize the cryptor and API

    let res = backend_api.chain_default_list().await.unwrap();

    println!("[test_chain_default_list] res: {res:?}");

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    println!("[test_chain_default_list] res_str: {res_str:?}");

    Ok(())
}

#[tokio::test]
async fn _chain_list() -> Result<(), Error> {
    let backend_api = init()?; // Initialize the cryptor and API

    let res = backend_api._chain_list().await.unwrap();

    println!("[_chain_list] res: {res:?}");

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    println!("[_chain_list] res_str: {res_str:?}");

    Ok(())
}

#[tokio::test]
async fn test_chain_list() -> Result<(), Error> {
    // init_test_log();
    let backend_api = init()?; // Initialize the cryptor and API

    let req = ChainListReq::new("1.4.1".to_string());
    let res = backend_api.chain_list(req).await.unwrap();

    tracing::info!("[test_chain_list] res: {res:?}");

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("[test_chain_list] res_str: {res_str:?}");

    Ok(())
}

#[tokio::test]
async fn test_chain_rpc_list() -> Result<(), Error> {
    // init_test_log();
    let backend_api = init()?; // Initialize the cryptor and API

    let chain_code = vec!["eth".to_string(), "tron".to_string(), "btc".to_string()];
    let req = ChainRpcListReq::new(chain_code);

    let res = backend_api.chain_rpc_list(req).await.unwrap();

    tracing::info!("[test_chain_rpc_list] res: {res:?}");

    let res_str = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("[test_chain_rpc_list] res_str: {res_str:?}");

    Ok(())
}
