use crate::tron::get_chain;
use wallet_chain_interact::tron::operations::{self, TronConstantOperation, TronTxOperation};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_utils::unit;

#[tokio::test]
async fn test_balance() {
    let instance = get_chain();
    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    // let token = Some("TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj".to_string());
    let token: Option<String> = None;

    let res = instance.balance(address, token).await;
    tracing::info!("balance of = {:?}", res);
    assert!(res.is_ok())
}

#[tokio::test]
async fn test_estimate_fee() {
    let from = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";
    let to = "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N";
    let value = unit::convert_to_u256("53.075021", 6).unwrap();
    let memo = None;
    // let memo = Some("test".to_string());

    let params = operations::transfer::TransferOpt::new(from, to, value, memo).unwrap();
    let res = get_chain()
        .simulate_simple_fee(from, to, 1, params)
        .await
        .unwrap();

    tracing::info!("brand consumer = {:?}", res);
    tracing::info!("transaction fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn test_estimate_token_fee() {
    // let contract = "TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj";
    let contract = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("1", 6).unwrap();
    let memo = Some("test".to_string());

    let params =
        operations::transfer::ContractTransferOpt::new(&contract, from, to, value, memo).unwrap();

    let res = get_chain().contract_fee(&from, 1, params).await.unwrap();

    tracing::info!("brand consumer = {:#?}", res);
    tracing::info!("transaction fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn test_transfer() {
    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("1", 6).unwrap();
    // let memo = "test".to_string();
    // let memo = "test".to_string();

    let params = operations::transfer::TransferOpt::new(from, to, value, None).unwrap();

    let key = "623b9474906c1e5b4d98d89aaf3ff6ff9597f847ee890a84379dcfb7a6a8a878";
    let key = ChainPrivateKey::from(key);

    let chain = get_chain();
    let raw = params
        .build_raw_transaction(chain.get_provider())
        .await
        .unwrap();
    let instance = chain.exec_transaction_v1(raw, key).await.unwrap();

    tracing::info!("tx info of = {:?}", instance);
}

#[tokio::test]
async fn test_token_transfer_fee() {
    let contract = "TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj";
    // let contract = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("0.1", 6).unwrap();
    let memo = Some("test".to_string());

    let params =
        operations::transfer::ContractTransferOpt::new(&contract, &from, &to, value, memo).unwrap();

    let res = get_chain().contract_fee(&from, 1, params).await.unwrap();

    tracing::info!("brand consumer = {:#?}", res);
    tracing::info!("transaction fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn test_token_transfer() {
    let contract = "TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj";

    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("0.1", 6).unwrap();
    let memo = Some("test".to_string());

    let mut params =
        operations::transfer::ContractTransferOpt::new(&contract, &from, &to, value, memo).unwrap();

    let key = "623b9474906c1e5b4d98d89aaf3ff6ff9597f847ee890a84379dcfb7a6a8a878";
    let key = ChainPrivateKey::from(key);

    let chain = get_chain();
    let c = params
        .constant_contract(chain.get_provider())
        .await
        .unwrap();

    let fee = chain.provider.contract_fee(c, 1, &from).await.unwrap();
    params.set_fee_limit(fee);

    let raw = params
        .build_raw_transaction(chain.get_provider())
        .await
        .unwrap();
    let tx = chain.exec_transaction_v1(raw, key).await.unwrap();

    tracing::info!("tx info of = {:?}", tx);
}

#[tokio::test]
async fn test_decimals() {
    let instance = get_chain();

    let token = "TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj";

    let res = instance.decimals(token).await.unwrap();
    tracing::info!("decimals = {:?}", res);

    let name = instance.token_name(token).await.unwrap();
    tracing::info!("name = {:?}", name);

    let symbol = instance.token_symbol(token).await.unwrap();
    tracing::info!("symbol = {:?}", symbol);
}

#[tokio::test]
async fn test_query_tx() {
    let instance = get_chain();
    let tx = "53551f65e23d49b85a9f665c80a450dfe07325a9e131a5c4e88d0ce55dfd9770";
    let res = instance.query_tx_res(tx).await;
    tracing::info!("tx info of = {:?}", res);
    assert!(res.is_ok())
}

#[tokio::test]
async fn test_account_resource() {
    let instance = get_chain();
    let res = instance
        .provider
        .account_resource("TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1")
        .await
        .unwrap();
    tracing::info!("tx info of = {:#?}", res);
}

#[tokio::test]
pub async fn test_parameter() {
    let chain = get_chain();
    let parameter = chain.provider.chain_params().await.unwrap();

    // let fee = parameter.get_create_account_fee();
    let fee = parameter.get_multi_sign_fee();
    tracing::info!("create account fee {:?}", fee)
}

#[tokio::test]
pub async fn test_block_height() {
    let chain = get_chain();
    let block = chain.provider.get_block().await.unwrap();
    tracing::info!("block height {:?}", block.block_header.raw_data.number)
}

#[tokio::test]
pub async fn test_chain_params() {
    let chain = get_chain();
    let parameter = chain.provider.chain_params().await.unwrap();
    tracing::info!("block height {:#?}", parameter)
}

#[tokio::test]
pub async fn test_all() {
    let chain = get_chain();

    let from = "TAU1t14o8zZksWRKjwqAVPTMXczUZzvMR1";
    let to = "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N";
    let value = unit::convert_to_u256("53.075021", 6).unwrap();

    let params =
        operations::transfer::TransferOpt::new(from, to, value, Some("sss".to_string())).unwrap();

    let provider = chain.get_provider();
    let tx = params.build_raw_transaction(provider).await.unwrap();
    let consumer = provider
        .transfer_fee(&params.from, Some(&params.to), &tx, 1)
        .await
        .unwrap();

    let account = provider.account_info(&params.from).await.unwrap();

    tracing::warn!("consumer {:#?}", consumer);
    tracing::warn!("consumer {:#?}", account);

    // if account.balance < consumer.transaction_fee_i64() + value_i64 {
    //     return Err(crate::BusinessError::Chain(
    //         crate::ChainError::InsufficientBalance,
    //     ))?;
    // }

    // let bill_consumer = BillResourceConsume::new_tron(consumer.bandwidth.consumer as u64, 0);

    // let tx_hash = chain.exec_transaction_v1(tx, private_key).await?;
    // Ok((tx_hash, bill_consumer.to_json_str()?))
}

#[tokio::test]
async fn test_black_address() {
    let chain = get_chain();

    let token = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
    // let owner = "TKZnKVp8A5SPH9qkHZyh9JKa9Tj4kW5V5h";
    let owner = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1";

    let res = chain.black_address(token, owner).await.unwrap();
    tracing::warn!("is black address {}", res);
}
