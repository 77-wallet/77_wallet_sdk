use crate::sol::get_chain;
use wallet_chain_interact::sol::operations::{self, SolInstructionOperation};

#[tokio::test]
async fn test_multi_address() {
    let res = operations::multisig::account::MultisigAccountOpt::multisig_address().unwrap();

    // authority_address: "Hq2GAPeHRWNPB1zKBQvEjpdg2JghYxa62XexiPfe5paF",
    // multisig_address: "6aWS5hYahESg4UeuyU15sME5bSAWFcanb8UsH98h3xet",
    // salt: "3MHkom8FTNyHFD4b96GBSy2oyJ2KdKVAMadcVaZVB8AmtWbR9k6NgtBXRdUeKbxEkx8ENMVkLFhpjj3W25kq4EXK",

    println!("multisig address {res:?}")
}

fn get_owners() -> Vec<String> {
    vec![
        "MmqgDWhS59oXWVuVtogpvj6k5RLny2ZHCGwDQX1yqkC".to_string(),
        "Ey3PmUxYJXK6DrtNSq47aE86tcMf9u6EbM89Dh76etPt".to_string(),
        "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_string(),
    ]
}

#[tokio::test]
async fn test_deploy_fee() {
    let instance = get_chain();
    let from = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let salt =
        "3MHkom8FTNyHFD4b96GBSy2oyJ2KdKVAMadcVaZVB8AmtWbR9k6NgtBXRdUeKbxEkx8ENMVkLFhpjj3W25kq4EXK"
            .to_string();

    let params = operations::multisig::account::MultisigAccountOpt::new(
        &from,
        2,
        get_owners(),
        salt,
        instance.get_provider(),
    )
    .unwrap();
    let instructions = params.instructions().await.unwrap();

    let rs = instance
        .estimate_fee_v1(&instructions, &params)
        .await
        .unwrap();

    tracing::info!("deploy account {:?}", rs.transaction_fee());
}

#[tokio::test]
async fn test_deploy_multisig() {
    let instance = get_chain();
    let from = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";

    let create_key = solana_sdk::signature::Keypair::new();
    let salt = create_key.to_base58_string();

    let params = operations::multisig::account::MultisigAccountOpt::new(
        &from,
        2,
        get_owners(),
        salt,
        instance.get_provider(),
    )
    .unwrap();
    let key =
        "PhKgs4sb76HtfzZv2N5ZxFfupPtKQghRdE3c8q2UW65JokknVwtPvsGnzQYtURAtf6Z5u1DFVtNxqzwkMJJ7VwQ";

    let instructions = params.instructions().await.unwrap();
    let res = instance
        .exec_transaction(params, key.into(), None, instructions, 0)
        .await
        .unwrap();

    // simulate transaction

    // let instructions = params.instructions().await.unwrap();
    // let payer = address::parse_sol_address(&from).unwrap();
    // let key = solana_sdk::signature::Keypair::from_base58_string(&key);
    // let other = params.other_keypair();
    // let mut keypair = vec![];
    // if !other.is_empty() {
    //     keypair.extend(&other);
    // }
    // keypair.push(&key);
    // let res = instance
    //     .get_provider()
    //     ._simulate_transaction(instructions, &payer, &keypair)
    //     .await
    //     .unwrap();

    // let res = instance.transfer(params, key.into(), None).await.unwrap();

    tracing::info!("deploy account {}", res);
}

#[tokio::test]
async fn test_create_multi_transfer() {
    let chain = get_chain();

    let from = "6aWS5hYahESg4UeuyU15sME5bSAWFcanb8UsH98h3xet";
    let to = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let value = "0.001";
    let decimal = 9;
    // let token = Some("C49WUif5gXpCyHqv391VUZB6E9QRfQrF7CGnyFVtwbAB".to_string());
    let token = None;

    let params = operations::transfer::TransferOpt::new(
        from,
        to,
        value,
        token,
        decimal,
        chain.get_provider(),
    )
    .unwrap();

    let creator = "4tyeH6KgV2ZHsE7D4ctxT2wpfqYqe5aMM7VJABqaQ3H9";
    let multisig_pda = "5eZDNHgRF5DxX52GL8dE5SfPXS2NK9PqmWwXCvyS8mrd";
    let multisig =
        operations::multisig::transfer::BuildTransactionOpt::new(multisig_pda, 3, creator, params)
            .unwrap();

    // 获取交易参数
    let args = multisig.build_transaction_arg().await.unwrap();
    // 交易指令
    let instructions = multisig.instructions(&args).await.unwrap();

    // 预估手续费
    let base_fee = chain
        .estimate_fee_v1(&instructions, &multisig)
        .await
        .unwrap();

    let _extra = multisig
        .create_transaction_fee(&args.transaction_message, base_fee)
        .await
        .unwrap();
    // tracing::info!("fee {:?}", _extra.transaction_fee());
    let pda = multisig.multisig_pda.clone();
    let key =
        "3f4aZH2YBMqeydTcwcVw3qZCgTRcDLvVt4rmCVtz6Apbisdsr6ftXPtsHdEZH1Wqm9kQd6eL66ExD1sadS939qdH";

    let c = chain
        .exec_transaction(multisig, key.into(), None, instructions, 0)
        .await
        .unwrap();
    let resp = args.get_raw_data(pda, c).unwrap();

    // let rs = chain.build_multisig_tx(multisig, key.into()).await.unwrap();
    tracing::info!("tx hash ={:?}", resp);
}

#[tokio::test]
async fn test1_sign_transaction() {
    let signer = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let key =
        "PhKgs4sb76HtfzZv2N5ZxFfupPtKQghRdE3c8q2UW65JokknVwtPvsGnzQYtURAtf6Z5u1DFVtNxqzwkMJJ7VwQ";
    let data = "+ghLwsFUURVscNRyXd2HRF7roSS06PUd0qY/SXSis9QCAAAAAAAAAAABAABBQUM0QUFBQUFRRUNCVkxmNFhTN3JOZ0hEZWo4ZUVKVXQ0N1dsRVA0MU9sWHJJZ283ck1XZTNlQlJISk5OcU9SbnNkK1NlVDA2bEVENWlEcFFOYTh0UGV4YjFtVFN2R040R2pxbk4rMjBKcXFNRVpWb0hWellNajBwY1NIY3lWVThLTUkyU1RxbWdDN1BBYmQ5dUhYWmFHVDJjdmhSczdyZWF3Y3RJWHRYMXMza1RxTTlZVisvd0NwcEQ3clBmaWVYdFFwVm1KSVFRY2Vya0l0Y1hTVzVOUzZVWVgwN1AzNTczd0JBd1FCQkFJQUNnQU1RRXRNQUFBQUFBQUdBQUE9";
    let params =
        operations::multisig::transfer::SignTransactionOpt::new(signer, data.to_string()).unwrap();

    let instructions = params.instructions().await.unwrap();
    let res = get_chain()
        .sign_with_res(instructions, params, key.into())
        .await
        .unwrap();

    tracing::info!("sing res {:?}", res);
}

#[tokio::test]
async fn test2_sign_transaction() {
    let signer = "MmqgDWhS59oXWVuVtogpvj6k5RLny2ZHCGwDQX1yqkC";
    let key =
        "2RzvCMfA9oGyRdc7pmX4WUh7KUEedCzTgWZ339vx6x1GYiywBDB6ig5VfPKoiGadEKRaBHRFKDi8zYyFJiJuFrmW";
    let data = "+ghLwsFUURVscNRyXd2HRF7roSS06PUd0qY/SXSis9QCAAAAAAAAAAABAABBQUM0QUFBQUFRRUNCVkxmNFhTN3JOZ0hEZWo4ZUVKVXQ0N1dsRVA0MU9sWHJJZ283ck1XZTNlQlJISk5OcU9SbnNkK1NlVDA2bEVENWlEcFFOYTh0UGV4YjFtVFN2R040R2pxbk4rMjBKcXFNRVpWb0hWellNajBwY1NIY3lWVThLTUkyU1RxbWdDN1BBYmQ5dUhYWmFHVDJjdmhSczdyZWF3Y3RJWHRYMXMza1RxTTlZVisvd0NwcEQ3clBmaWVYdFFwVm1KSVFRY2Vya0l0Y1hTVzVOUzZVWVgwN1AzNTczd0JBd1FCQkFJQUNnQU1RRXRNQUFBQUFBQUdBQUE9";
    let params =
        operations::multisig::transfer::SignTransactionOpt::new(signer, data.to_string()).unwrap();

    let instructions = params.instructions().await.unwrap();
    let res = get_chain()
        .sign_with_res(instructions, params, key.into())
        .await
        .unwrap();

    tracing::info!("sing res {:?}", res);
}

#[tokio::test]
async fn test_exec_multi_transaction() {
    let executor = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let keypair =
        "PhKgs4sb76HtfzZv2N5ZxFfupPtKQghRdE3c8q2UW65JokknVwtPvsGnzQYtURAtf6Z5u1DFVtNxqzwkMJJ7VwQ";
    let raw_data = "+ghLwsFUURVscNRyXd2HRF7roSS06PUd0qY/SXSis9QCAAAAAAAAAAABAABBQUM0QUFBQUFRRUNCVkxmNFhTN3JOZ0hEZWo4ZUVKVXQ0N1dsRVA0MU9sWHJJZ283ck1XZTNlQlJISk5OcU9SbnNkK1NlVDA2bEVENWlEcFFOYTh0UGV4YjFtVFN2R040R2pxbk4rMjBKcXFNRVpWb0hWellNajBwY1NIY3lWVThLTUkyU1RxbWdDN1BBYmQ5dUhYWmFHVDJjdmhSczdyZWF3Y3RJWHRYMXMza1RxTTlZVisvd0NwcEQ3clBmaWVYdFFwVm1KSVFRY2Vya0l0Y1hTVzVOUzZVWVgwN1AzNTczd0JBd1FCQkFJQUNnQU1RRXRNQUFBQUFBQUdBQUE9";

    let params =
        operations::multisig::transfer::ExecMultisigOpt::new(executor, raw_data.to_string())
            .unwrap();

    let instructions = params.instructions().await.unwrap();

    let res = get_chain()
        .exec_transaction(params, keypair.into(), None, instructions, 0)
        .await
        .unwrap();

    tracing::info!("get transaction hash = {:?}", res);
}

// #[tokio::test]
// async fn test_estimate_multisig_fee() {
//     let instance = get_chain();
//     let from = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6".to_string();
//     let key =
//         "PhKgs4sb76HtfzZv2N5ZxFfupPtKQghRdE3c8q2UW65JokknVwtPvsGnzQYtURAtf6Z5u1DFVtNxqzwkMJJ7VwQ"
//             .to_string();
//     let params = MultiSigAccount::new(from, 2, get_owners()).with_key(key);
//     let _fee = instance.deploy_multisig_fee(&params).await.unwrap();
// }
