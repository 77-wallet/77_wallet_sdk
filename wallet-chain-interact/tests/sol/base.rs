use crate::sol::get_chain;
use wallet_chain_interact::sol::operations::{
    self,
    multisig::{pda, MULTISIG_PROGRAM_ID},
    SolInstructionOperation,
};
use wallet_utils::address;

#[tokio::test]
async fn test_balance() {
    let instance = get_chain();

    let addr = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    // let addr = "MmqgDWhS59oXWVuVtogpvj6k5RLny2ZHCGwDQX1yqkC";
    // let token = Some("C49WUif5gXpCyHqv391VUZB6E9QRfQrF7CGnyFVtwbAB".to_string());
    let token: Option<String> = None;
    let balance = instance.balance(addr, token).await.unwrap();

    tracing::info!(
        "balance = {:?}",
        wallet_utils::unit::format_to_string(balance, 9)
    );
}

#[tokio::test]
async fn test_transfer() {
    let chain = get_chain();

    let from = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let to = "2t2bb63CcxSE6gWZHvAHc6q24ub9vyoWFEKuxqALkyfX";
    let value = "0.1";
    let decimal = 9;
    // let token = Some("C49WUif5gXpCyHqv391VUZB6E9QRfQrF7CGnyFVtwbAB".to_string());
    let token = None;

    let key =
        "PhKgs4sb76HtfzZv2N5ZxFfupPtKQghRdE3c8q2UW65JokknVwtPvsGnzQYtURAtf6Z5u1DFVtNxqzwkMJJ7VwQ";

    let params = operations::transfer::TransferOpt::new(
        from,
        to,
        value,
        token,
        decimal,
        chain.get_provider(),
    )
    .unwrap();

    let instructions = params.instructions().await.unwrap();
    let rs = chain
        .exec_transaction(params, key.into(), None, instructions, 10)
        .await
        .unwrap();

    // let rs = chain.transfer(params, key.into(), None).await.unwrap();
    tracing::info!("tx hash {}", rs);
}

#[tokio::test]
async fn transfer_fee() {
    let chain = get_chain();

    let from = "Auc6pBz9AdxmTQfCMKjuyacR2BtipNeXoLz5Rg7LgY8C";
    let to = "DNi5Byx5KAbYSXyajgEc5n7pidDJG2PDh62L4ncY9RcL";
    let value = "10";
    let decimal = 6;
    let token = Some("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string());
    // let token = None;

    let params = operations::transfer::TransferOpt::new(
        from,
        to,
        value,
        token,
        decimal,
        chain.get_provider(),
    )
    .unwrap();

    let instructions = params.instructions().await.unwrap();
    let rs = chain.estimate_fee_v1(&instructions, &params).await.unwrap();
    tracing::info!("fee ={:?}", rs);
    tracing::info!("transaction fee ={}", rs.transaction_fee());
}

#[tokio::test]
async fn query_tx() {
    let instance = get_chain();

    let txid =
        "1rPH42QgM7D3UJRhmMPXi1cHwLHXhCa6vJ3NwELzVZ75WPG86oqhEDknFkoaocV2pX2EmJ9a1Y3LmA9EfyiWoBq";
    let tx_info = instance.query_tx_res(&txid).await.unwrap();
    // let tx_info = instance
    //     .get_provider()
    //     .query_transaction(&txid)
    //     .await
    //     .unwrap();

    tracing::info!("tx_info = {:?}", tx_info);
    // assert!(tx_info.is_ok())
}

#[tokio::test]
async fn test_decimals() {
    let instance = get_chain();

    let token = "C49WUif5gXpCyHqv391VUZB6E9QRfQrF7CGnyFVtwbAB";
    // let addr = "GE93MHXVvnsbhxu7Ttpp7zTiipJeCX3QFXueSK2dCJe6";
    let res = instance.decimals(&token).await;

    tracing::info!("tx_info = {:?}", res);
    assert!(res.is_ok())
}

#[tokio::test]
async fn test_per_signature() {
    let instance = get_chain();
    let res = instance.per_signature_fee().await.unwrap();
    tracing::info!("tx_info = {:?}", res);
}

#[tokio::test]
async fn test_config_program() {
    let instance = get_chain();

    let program_id = address::parse_sol_address(MULTISIG_PROGRAM_ID).unwrap();
    let config_pda = pda::get_program_config_pda(&program_id);

    tracing::info!("config {:?}", config_pda);
    let res = instance
        .get_provider()
        .get_config_program(&config_pda.0)
        .await
        .unwrap();

    tracing::info!("tx_info = {:?}", res);
}

#[tokio::test]
async fn test_parse_block() {
    let slot = 296046701;
    // let slot = 295744676;
    let time1 = std::time::Instant::now();
    let chain = get_chain().get_provider().get_block(slot).await.unwrap();
    let duration = time1.elapsed();
    tracing::warn!("request duration: {:?}", duration);

    tracing::info!("tx_info = {:?}", chain.transactions.len());
}

#[tokio::test]
async fn test_get_slot() {
    let chain = get_chain().get_provider().get_slot().await.unwrap();
    tracing::info!("tx_info = {:?}", chain);
}

#[tokio::test]
async fn test_black_address() {
    let chain = get_chain();

    let token = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";
    // let owner = "6Hsn4noVtYMvCauR6wrM5JMmLAkfucoLF9koTq4P1Awf";
    let owner = "HWxpv68HZQgHEFAAs3petNynCc7iSj6fFRYax4Drvcqb";

    let res = chain.black_address(token, owner).await.unwrap();

    tracing::info!("{res:?}");
}

#[tokio::test]
async fn test_retry_transactrion() {
    let chain = get_chain();

    let provider = chain.get_provider();

    let tx_hash =
        "5fsxQeC2fLbpBoHUucecU6gzoKuvvwkMCYn4rNntLx7bBhN1EJLjQefHzHPo8YrDqmB3BJ5G76VRQWDhGZtFTDTY";
    let res = provider.get_signature_status(tx_hash).await.unwrap();

    tracing::info!("{res:?}");
}
