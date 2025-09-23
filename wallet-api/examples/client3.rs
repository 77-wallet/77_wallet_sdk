use tokio_stream::StreamExt as _;
use wallet_api::{
    manager::WalletManager,
    messaging::notify::FrontendNotifyEvent,
    test::env::{TestParams, get_manager},
};
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::request::api_wallet::strategy::{ChainConfig, IndexAndAddress};
use wallet_types::chain::chain::ChainCode;
// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

async fn run(
    wallet_manager: &WalletManager,
    test_params: &TestParams,
) -> Result<(), Box<dyn std::error::Error>> {
    // 创建钱包
    let language_code = 1;
    let phrase = &test_params.create_wallet_req.phrase;
    let salt = "q1111111";
    let wallet_name = "api_wallet";
    let account_name = "ccccc";
    let is_default_name = true;
    let wallet_password = "q1111111";
    let wallet_uid = wallet_manager
        .create_api_wallet(
            language_code,
            phrase,
            salt,
            wallet_name,
            account_name,
            is_default_name,
            wallet_password,
            None,
            ApiWalletType::SubAccount,
        )
        .await?;
    tracing::warn!("wallet ------------------------ 1: {wallet_uid:#?}");

    let salt1 = "q1111112";
    let withdrawal_uid = wallet_manager
        .create_api_wallet(
            language_code,
            phrase,
            salt1,
            wallet_name,
            account_name,
            is_default_name,
            wallet_password,
            None,
            ApiWalletType::Withdrawal,
        )
        .await?;
    tracing::warn!("wallet ------------------------ 2: {withdrawal_uid:#?}");

    let res = wallet_manager
        .bind_merchant(
            "68c27fb92e52f46cef896318",
            "68be7271a7307e042404e276",
            &wallet_uid,
            &withdrawal_uid,
        )
        .await?;
    tracing::info!("bind_merchant ------------------- 3: {res:#?}");

    let res = wallet_manager
        .update_collect_strategy(
            &wallet_uid,
            1.1,
            vec![ChainConfig {
                chain_code: ChainCode::Tron.to_string(),
                normal_address: IndexAndAddress {
                    index: Some(0),
                    address: "TLAedgzGJWA9seJYbBTTMWNtxoKooapq6n".to_string(),
                },
                risk_address: IndexAndAddress {
                    index: Some(1),
                    address: "TNoacEYG6dCB2z9aWPVYspz1qrxHDoe8Bv".to_string(),
                },
            }],
        )
        .await;
    match res {
        Ok(_) => {
            tracing::info!("wallet --------------------- 4: {res:#?}");
        }

        _ => {
            tracing::error!("wallet --------------------- 5: {res:#?}");
        }
    }

    // 获取订单记录
    // let order_list = wallet_manager.list_api_withdraw_order(&wallet_uid).await?;
    // tracing::info!("order_list ------------------- 2: {order_list:#?}");

    // 绑定钱包
    // let key = "app_id";
    // let merchain_id = "test_merchain";

    //
    // let res = wallet_manager.bind_merchant(key, merchain_id, uid).await;
    // tracing::info!("res --------------------- 3: {res:?}");

    // bnb
    // let from = "0x4f31D44C05d6fDce4db64da2E9601BeE8ad9EA5e";
    // let to = "0xF97c59fa5C130007CF51286185089d05FE45B69e";

    // tron
    // let from = "TLAedgzGJWA9seJYbBTTMWNtxoKooapq6n";
    // let to = "TNRUkgGzhwuRL2rGeFPErThYWr4MranYLA";

    // sol
    // let from = "DF3Nong1byLe4Nb1Qu4R8T4G7TFDpLe7T58moGbUotpe";
    // let to = "J8ByH2pUySpXL4fXdgPpwnaL7R381xunqXT2cqaZ1tm";

    // ton
    // let from = "UQBTmOIHin7OrxheQ979Y3_xJjHxMUJocknrv3_J_dCocuqy";
    // let to = "0QDex-zBG6cbJCwaxA7999xIB_ZhNAwOr37lsw5HxB7Ldrpq";

    // sui
    // let from = "0xb69713b670ba3bfcfa7ea577005de40bf026e277b574773bc4c6f0adb7e1ced8";
    // let to = "0xd830497ecd7321d4e0e501d3f71689380e8e8883ee5e1597cf06b3b72a95d226";

    // let value = "0.000001";
    // let trade_no = "0x0000000125";
    // let res1 = wallet_manager
    //     .api_withdrawal_order(from, to, value, "bnb", None, "BNB", trade_no, 1, &wallet_uid)
    //     .await;
    // tracing::info!("api_withdrawal_order ------------------- 4: {res1:#?}");
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();
    let (wallet_manager, test_params) = get_manager().await?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    wallet_manager.set_frontend_notify_sender(tx).await?;

    wallet_manager.init(test_params.device_req.clone()).await?;

    let res = wallet_manager.set_invite_code(Some("I1912683353004912640".to_string())).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("set_invite_code ------------------------0: {res:?}");

    let res = run(&wallet_manager, &test_params).await;
    match res {
        Ok(_) => {}
        Err(err) => {
            tracing::error!(" =========================== run {}", err)
        }
    }

    // let topics = vec![
    //     "wallet/token/eth/usdc".to_string(),
    //     "wallet/token/tron/trx".to_string(),
    //     "wallet/token/doge/doge".to_string(),
    //     "wallet/token/tron/sun".to_string(),
    //     "wallet/token/tron/win".to_string(),
    //     "wallet/token/eth/hkby".to_string(),
    //     "wallet/token/btc/btc".to_string(),
    //     "wallet/token/eth/eth".to_string(),
    //     "wallet/token/bnb/bnb".to_string(),
    //     "wallet/token/sol/sol".to_string(),
    //     "wallet/token/ltc/ltc".to_string(),
    //     "wallet/token/eth/link".to_string(),
    //     "wallet/token/ton/ton".to_string(),
    //     "wallet/token/sui/sui".to_string(),
    //     "wallet/token/eth/cake".to_string(),
    //     "wallet/token/sol/usdt".to_string(),
    // ];
    // {
    //     wallet_manager.mqtt_subscribe(topics, None).await;
    // }

    // let sync_res = wallet_manager.sync_assets(vec![], None, vec![]).await;
    // tracing::info!("sync res: {sync_res:#?}");
    // let wallet = wallet.unwrap();
    // test_params.create_account_req.wallet_address = wallet.address.clone();

    // let config = wallet_manager.get_config().await;
    // tracing::info!("config result: {config:#?}");
    // let res = wallet_utils::serde_func::serde_to_string(&config)?;
    // tracing::info!("config result: {res}");
    loop {
        tokio::select! {
            msg = rx.next() => {
                tracing::info!("data: {msg:?}");
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("ctrl_c");
                let _ = wallet_manager.close().await;
                break;
            }
        }
    }
    Ok(())
}
