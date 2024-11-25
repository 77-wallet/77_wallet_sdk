use crate::btc::get_chain;
use wallet_chain_interact::btc::operations;
use wallet_chain_interact::types::ChainPrivateKey;

#[tokio::test]
async fn test_balance() {
    let addr = "bcrt1qavm69e9xuqme0w0x752sl2lj5zjw2e637eshgc";
    let balance = get_chain().balance(addr, None).await;

    tracing::info!("balance = {:?}", balance);
    assert!(balance.is_ok());

    let balance = wallet_utils::unit::format_to_string(balance.unwrap(), 8).unwrap();
    tracing::info!("balance = {:?}", balance);
}

#[tokio::test]
async fn test_estimate_fee_v1() {
    let instance = get_chain();
    let network = instance.network;
    let to = "n2xfjp4NfSMWao3V119b5JEU3CKZ7jDZAK";
    let value = "5";

    // p2pkh
    // let from = "n2xfjp4NfSMWao3V119b5JEU3CKZ7jDZAK";
    // let params = operations::transfer::TransferArg::new(from, to, value, "p2pkh", network).unwrap();
    // let tx = instance.estimate_fee_v1(params).await.unwrap();
    // tracing::info!("fee = {:?}", tx);
    // tracing::info!("fee = {:?}", tx.transaction_fee());

    // p2sh-wpkh
    // let from = "2N3MreEe3z9SFKobZukoxzQgWd4gzqkTWru";
    // let params = operations::transfer::TransferArg::new(from, to, value, "p2sh-wpkh", network)
    //     .unwrap()
    //     .with_spend_all(true);
    // let tx = instance.estimate_fee(params).await.unwrap();
    // tracing::info!("fee = {:?}", tx);
    // tracing::info!("fee = {:?}", tx.transaction_fee());

    // p2wpkh
    // let from = "bcrt1qjx3d2sfu5v0jykpzs3a668nf26cgh9awsh7ek9";
    // let params =
    //     operations::transfer::TransferArg::new(from, to, value, "p2wpkh", network).unwrap();
    // let tx = instance.estimate_fee_v1(params).await.unwrap();
    // tracing::info!("fee = {:?}", tx);
    // tracing::info!("fee = {:?}", tx.transaction_fee());

    // p2tr
    let from = "bcrt1pzncrlhnm3qk92xtkwy9n3vjy44sc48jlmvaz6sjewr8cm0y7rnks2kt636";
    let params =
        operations::transfer::TransferArg::new(from, to, value, Some("p2tr".to_string()), network)
            .unwrap()
            .with_spend_all(true);
    let tx = instance.estimate_fee(params).await.unwrap();

    tracing::info!("fee = {:?}", tx);
    // tracing::info!("fee = {:?}", tx.transaction_fee());
}

#[tokio::test]
async fn test_transfer_v1() {
    let instance = get_chain();
    let network = instance.network;
    let to = "n2xfjp4NfSMWao3V119b5JEU3CKZ7jDZAK";
    let value = "1000";

    // p2pkh
    // let from = "n2xfjp4NfSMWao3V119b5JEU3CKZ7jDZAK";
    // let key = ChainPrivateKey::from("cVhhLRum8YEgvaA3BChwTBBBYYr8QkLWLgb7Rri3SiZkbLUEX4Et");
    // let params = operations::transfer::TransferArg::new(from, to, value, "p2pkh", network)
    //     .unwrap()
    //     .with_spend_all(true);
    // let tx = instance.transfer(params, key).await.unwrap();
    // tracing::info!("tx_hash = {:?}", tx);

    // p2sh-wpkh
    // let from = "2N3MreEe3z9SFKobZukoxzQgWd4gzqkTWru";
    // let key = ChainPrivateKey::from("cVhhLRum8YEgvaA3BChwTBBBYYr8QkLWLgb7Rri3SiZkbLUEX4Et");
    // let params = operations::transfer::TransferArg::new(from, to, value, "p2sh-wpkh", network)
    //     .unwrap()
    //     .with_spend_all(true);
    // let tx = instance.transfer(params, key).await.unwrap();
    // tracing::info!("tx_hash = {:?}", tx);

    // p2wpkh
    let from = "bcrt1qjx3d2sfu5v0jykpzs3a668nf26cgh9awsh7ek9";
    let key = ChainPrivateKey::from("cT9bnaLgcNHRx7FwnxVwLtk87XAvrukv4ppjUdFPeoTJ1hYzjqta");
    let params = operations::transfer::TransferArg::new(
        from,
        to,
        value,
        Some("p2wpkh".to_string()),
        network,
    )
    .unwrap();
    let tx = instance.transfer(params, key).await.unwrap();
    tracing::info!("tx_hash = {:?}", tx);

    // // p2tr
    // let from = "bcrt1pzncrlhnm3qk92xtkwy9n3vjy44sc48jlmvaz6sjewr8cm0y7rnks2kt636";
    // let key = ChainPrivateKey::from("cVhhLRum8YEgvaA3BChwTBBBYYr8QkLWLgb7Rri3SiZkbLUEX4Et");
    // let params = operations::transfer::TransferArg::new(from, to, value, "p2tr", network)
    //     .unwrap()
    //     .with_spend_all(true);
    // let tx = instance.transfer(params, key).await.unwrap();
    // tracing::info!("tx_hash = {:?}", tx);
}

#[tokio::test]
async fn test_query_tx() {
    let instance = get_chain();
    let txid = "e891615e3ee99edeb50dd2d1aff1ffe7e90402d052c0f0e802c51a2a40c9a57d";
    let tx_info = instance.query_tx_res(&txid).await;

    tracing::info!("tx_info = {:?}", tx_info);
    assert!(tx_info.is_ok())
}

#[tokio::test]
async fn test_parse_tx() {
    let tx = "221d010bcb3836fc37f87b6e35a6fa88c1424177f1f04450803e04617c3b7bd5";
    let chain = get_chain()
        .get_provider()
        .get_transaction_from_api(tx)
        .await;

    tracing::info!("tx = {:?}", chain);
}

#[tokio::test]
async fn test_parse_block() {
    let tx = "2904278";
    let chain = get_chain()
        .get_provider()
        .get_block_from_api(tx, 4)
        .await
        .unwrap();

    tracing::info!("tx = {:?} count = {}", chain.txs.len(), chain.tx_count);
}
