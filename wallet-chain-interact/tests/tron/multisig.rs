use crate::tron::get_chain;
use wallet_chain_interact::tron::operations::{multisig, transfer};
use wallet_chain_interact::tron::operations::{TronConstantOperation, TronTxOperation};
use wallet_utils::unit;

fn get_owners() -> Vec<String> {
    vec![
        // "TJk5nUGoaMFmcrmSubFD11w6DVf5uX5yi6".to_string(),
        "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn".to_string(),
    ]
}

#[tokio::test]
async fn test_deploy_fee() {
    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let params = multisig::MultisigAccountOpt::new(&from, 1, get_owners()).unwrap();

    let res = get_chain().simple_fee(&from, 1, params).await.unwrap();

    tracing::info!("brand consumer = {:?}", res);
    tracing::info!("deploy fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn test_deploy_multisig() {
    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let params = multisig::MultisigAccountOpt::new(&from, 1, get_owners()).unwrap();

    let res = get_chain().simple_fee(&from, 1, params).await.unwrap();

    tracing::info!("brand consumer = {:?}", res);
    tracing::info!("deploy fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn recover_multisig() {
    let from = "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ";
    let owners = vec![from.to_string()];
    let params = multisig::MultisigAccountOpt::new(&from, 1, owners).unwrap();
    let chain = get_chain();
    let raw = params.build_raw_transaction(&chain.provider).await.unwrap();

    let sign1 = wallet_utils::sign::sign_tron(
        &raw.tx_id,
        "e3d81eea6ea17bbd5d19c415c560bb25369f60720bc89b425735eb2b005c330c",
        None,
    )
    .unwrap();

    // tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    let sign2 = wallet_utils::sign::sign_tron(
        &raw.tx_id,
        "dc3f0906883e0c78dc6ce03bf34df9a23e38616dff2b76a8c8ab765a9f9cade9",
        None,
    )
    .unwrap();

    let sign = vec![sign1, sign2];
    let rs = chain.exec_multisig_transaction(raw, sign).await.unwrap();

    tracing::info!("multisig = {:?}", rs);
}

#[tokio::test]
async fn test_build_transaction() {
    let from = "TJuoqpfuVSxyjtobLP5Nb1oUMT8Fqew22j";
    let to = "TVx7Pi8Ftgzd7AputaoLidBR3Vb9xKfhqY";
    let value = unit::convert_to_u256("100", 6).unwrap();
    let memo = "test".to_string();

    let params = transfer::TransferOpt::new(from, to, value, Some(memo)).unwrap();
    let res = get_chain()
        .build_multisig_transaction(params, 3600 * 2)
        .await
        .unwrap();

    tracing::info!("multisig build = {:?}", res);
}

#[tokio::test]
async fn test_build_token_transaction() {
    let contract = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";

    let from = "TJuoqpfuVSxyjtobLP5Nb1oUMT8Fqew22j";
    let to = "TQHq9gP34tLiE2Eg1MeAQjhN6KA6oLRBos";
    let value = unit::convert_to_u256("4.703", 6).unwrap();
    let memo = None;

    let mut params =
        transfer::ContractTransferOpt::new(&contract, &from, &to, value, memo).unwrap();

    let chain = get_chain();
    let constant = params
        .constant_contract(chain.get_provider())
        .await
        .unwrap();

    let consumer = chain
        .get_provider()
        .contract_fee(constant, 2, from)
        .await
        .unwrap();
    params.set_fee_limit(consumer);

    let res = chain
        .build_multisig_transaction(params, 3600 * 2)
        .await
        .unwrap();

    tracing::info!("multisig build = {:?}", res);
}

#[tokio::test]
async fn test_sign_transaction1() {
    let raw_data = "QAAAAAAAAAA3NTc3NmE2MjQwNmZiNDdkYjVmMTViMDU2MmVkMDM3MjIxMWU0MGVmNjcwYWJlNjViNTVlODBhN2RlYjA3NTJheQEAAAAAAAB7ImNvbnRyYWN0IjpbeyJwYXJhbWV0ZXIiOnsidmFsdWUiOnsiYW1vdW50IjoxMDAwMDAwMDAsIm93bmVyX2FkZHJlc3MiOiI0MTYyMTQ4ZDQyZmZiNmU0Mjg5ZDQ2ZGY4Zjg0YzljM2E2YzQ5ZmUzN2QiLCJ0b19hZGRyZXNzIjoiNDFkYjJkNmFjYjYyN2E2MTVkNTQzZjAzNzkxMzE1ZjNjZDBkMjE2MmY1In0sInR5cGVfdXJsIjoidHlwZS5nb29nbGVhcGlzLmNvbS9wcm90b2NvbC5UcmFuc2ZlckNvbnRyYWN0In0sInR5cGUiOiJUcmFuc2ZlckNvbnRyYWN0In1dLCJyZWZfYmxvY2tfYnl0ZXMiOiJjZjc5IiwicmVmX2Jsb2NrX2hhc2giOiJjMWUyNzE4ZWVmYTMwNzUyIiwiZXhwaXJhdGlvbiI6MTczMjUzNjQwNTAwMCwidGltZXN0YW1wIjoxNzMyNTI5MTQ4MjQyfQwBAAAAAAAAMGEwMmNmNzkyMjA4YzFlMjcxOGVlZmEzMDc1MjQwODg5OGRlOTliNjMyNWE2ODA4MDExMjY0MGEyZDc0Nzk3MDY1MmU2NzZmNmY2NzZjNjU2MTcwNjk3MzJlNjM2ZjZkMmY3MDcyNmY3NDZmNjM2ZjZjMmU1NDcyNjE2ZTczNjY2NTcyNDM2ZjZlNzQ3MjYxNjM3NDEyMzMwYTE1NDE2MjE0OGQ0MmZmYjZlNDI4OWQ0NmRmOGY4NGM5YzNhNmM0OWZlMzdkMTIxNTQxZGIyZDZhY2I2MjdhNjE1ZDU0M2YwMzc5MTMxNWYzY2QwZDIxNjJmNTE4ODBjMmQ3MmY3MGQyYTJhMzk2YjYzMgAAAAAAAAAA";
    let private_key = "2ec979f82e99de3b5f21fbbabc77dbd27ef2c51b249ee1b294974a28357842b9";

    let sign = multisig::TransactionOpt::sign_transaction(raw_data, private_key.into()).unwrap();

    println!("sign = {:?}", sign);
}

#[tokio::test]
async fn test_sign_transaction2() {
    let raw_data = "QAAAAAAAAAA3NTc3NmE2MjQwNmZiNDdkYjVmMTViMDU2MmVkMDM3MjIxMWU0MGVmNjcwYWJlNjViNTVlODBhN2RlYjA3NTJheQEAAAAAAAB7ImNvbnRyYWN0IjpbeyJwYXJhbWV0ZXIiOnsidmFsdWUiOnsiYW1vdW50IjoxMDAwMDAwMDAsIm93bmVyX2FkZHJlc3MiOiI0MTYyMTQ4ZDQyZmZiNmU0Mjg5ZDQ2ZGY4Zjg0YzljM2E2YzQ5ZmUzN2QiLCJ0b19hZGRyZXNzIjoiNDFkYjJkNmFjYjYyN2E2MTVkNTQzZjAzNzkxMzE1ZjNjZDBkMjE2MmY1In0sInR5cGVfdXJsIjoidHlwZS5nb29nbGVhcGlzLmNvbS9wcm90b2NvbC5UcmFuc2ZlckNvbnRyYWN0In0sInR5cGUiOiJUcmFuc2ZlckNvbnRyYWN0In1dLCJyZWZfYmxvY2tfYnl0ZXMiOiJjZjc5IiwicmVmX2Jsb2NrX2hhc2giOiJjMWUyNzE4ZWVmYTMwNzUyIiwiZXhwaXJhdGlvbiI6MTczMjUzNjQwNTAwMCwidGltZXN0YW1wIjoxNzMyNTI5MTQ4MjQyfQwBAAAAAAAAMGEwMmNmNzkyMjA4YzFlMjcxOGVlZmEzMDc1MjQwODg5OGRlOTliNjMyNWE2ODA4MDExMjY0MGEyZDc0Nzk3MDY1MmU2NzZmNmY2NzZjNjU2MTcwNjk3MzJlNjM2ZjZkMmY3MDcyNmY3NDZmNjM2ZjZjMmU1NDcyNjE2ZTczNjY2NTcyNDM2ZjZlNzQ3MjYxNjM3NDEyMzMwYTE1NDE2MjE0OGQ0MmZmYjZlNDI4OWQ0NmRmOGY4NGM5YzNhNmM0OWZlMzdkMTIxNTQxZGIyZDZhY2I2MjdhNjE1ZDU0M2YwMzc5MTMxNWYzY2QwZDIxNjJmNTE4ODBjMmQ3MmY3MGQyYTJhMzk2YjYzMgAAAAAAAAAA";
    let private_key = "672b8d8c05082eea989ab4646442b0418a04fa6504f23897b8642a6a4693176c";

    let sign = multisig::TransactionOpt::sign_transaction(raw_data, private_key.into()).unwrap();

    println!("sign = {:?}", sign);
}

#[tokio::test]
async fn test_transaction_fee() {
    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("1", 6).unwrap();
    let memo = "test".to_string();

    let params = transfer::TransferOpt::new(from, to, value, Some(memo)).unwrap();
    let fee = get_chain().simple_fee(&from, 2, params).await.unwrap();

    tracing::info!("brand consumer = {:?}", fee);
    tracing::info!("transaction fee = {:?}", fee.transaction_fee());
}

#[tokio::test]
async fn test_token_transaction_fee() {
    let contract = "TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj";

    let from = "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ";
    let to = "TZ92GD6UbW8MMk6XD6pxKTGzUGs42No6vn";
    let value = unit::convert_to_u256("1", 6).unwrap();
    let memo = Some("test".to_string());

    let params = transfer::ContractTransferOpt::new(&contract, &from, &to, value, memo).unwrap();

    let res = get_chain().contract_fee(&from, 2, params).await.unwrap();

    tracing::info!("brand consumer = {:?}", res);
    tracing::info!("transaction fee = {:?}", res.transaction_fee());
}

#[tokio::test]
async fn test_execute() {
    let raw_data = "QAAAAAAAAAA3NTc3NmE2MjQwNmZiNDdkYjVmMTViMDU2MmVkMDM3MjIxMWU0MGVmNjcwYWJlNjViNTVlODBhN2RlYjA3NTJheQEAAAAAAAB7ImNvbnRyYWN0IjpbeyJwYXJhbWV0ZXIiOnsidmFsdWUiOnsiYW1vdW50IjoxMDAwMDAwMDAsIm93bmVyX2FkZHJlc3MiOiI0MTYyMTQ4ZDQyZmZiNmU0Mjg5ZDQ2ZGY4Zjg0YzljM2E2YzQ5ZmUzN2QiLCJ0b19hZGRyZXNzIjoiNDFkYjJkNmFjYjYyN2E2MTVkNTQzZjAzNzkxMzE1ZjNjZDBkMjE2MmY1In0sInR5cGVfdXJsIjoidHlwZS5nb29nbGVhcGlzLmNvbS9wcm90b2NvbC5UcmFuc2ZlckNvbnRyYWN0In0sInR5cGUiOiJUcmFuc2ZlckNvbnRyYWN0In1dLCJyZWZfYmxvY2tfYnl0ZXMiOiJjZjc5IiwicmVmX2Jsb2NrX2hhc2giOiJjMWUyNzE4ZWVmYTMwNzUyIiwiZXhwaXJhdGlvbiI6MTczMjUzNjQwNTAwMCwidGltZXN0YW1wIjoxNzMyNTI5MTQ4MjQyfQwBAAAAAAAAMGEwMmNmNzkyMjA4YzFlMjcxOGVlZmEzMDc1MjQwODg5OGRlOTliNjMyNWE2ODA4MDExMjY0MGEyZDc0Nzk3MDY1MmU2NzZmNmY2NzZjNjU2MTcwNjk3MzJlNjM2ZjZkMmY3MDcyNmY3NDZmNjM2ZjZjMmU1NDcyNjE2ZTczNjY2NTcyNDM2ZjZlNzQ3MjYxNjM3NDEyMzMwYTE1NDE2MjE0OGQ0MmZmYjZlNDI4OWQ0NmRmOGY4NGM5YzNhNmM0OWZlMzdkMTIxNTQxZGIyZDZhY2I2MjdhNjE1ZDU0M2YwMzc5MTMxNWYzY2QwZDIxNjJmNTE4ODBjMmQ3MmY3MGQyYTJhMzk2YjYzMgAAAAAAAAAA";

    let params = multisig::TransactionOpt::data_from_str(raw_data).unwrap();

    let sign1 = "4c857e4476a76ebb49ebcc4eaa85616199f512373a857210e3276161f996b49d25f2c5ac8aae8b2775f1c73c9fb230f9ba4469fea1c3d5a96ab3994f0bb4baaf00".to_string();
    let sign2 = "d0bd6b291fc0aa454c0d385b7a7ab0454a2bfb1bcc9ac6825457c0f30056286103fbee0552e79feda5c98b040af543a76198df40cd98713dffc5402d38c1f0fe00".to_string();

    let sign = vec![sign1, sign2];

    let res = get_chain().exec_multisig_transaction(params, sign).await;

    tracing::info!("res = {:?}", res);
}
