use std::sync::Arc;

use tokio_stream::StreamExt as _;
use wallet_api::{messaging::notify::FrontendNotifyEvent, test::env::get_manager};

// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();
    // wallet_api::WalletManager::init_log(Some("warn"))
    //     .await
    //     .unwrap();
    // Self::init_log(Some("error")).await?;
    let (wallet_manager, _test_params) = get_manager().await.unwrap();
    tracing::info!("init_api_swap");
    wallet_manager.init_api_swap().await?;
    let wallet_password = "q1111111";

    let _ = wallet_manager.set_passwd_cache(wallet_password).await;
    // wallet_api::WalletManager::init_log(Some("info"), "xxxx").await?;
    tracing::info!("set_frontend_notify_sender");
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    wallet_manager.set_frontend_notify_sender(tx).await?;
    // wallet_manager.init(test_params.device_req).await?;
    tracing::info!("set_invite_code");
    let res = wallet_manager.set_invite_code(Some("I1912683353004912640".to_string())).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("set_invite_code res: {res}");

    // // 创建钱包
    // let language_code = 1;
    // let phrase = &test_params.create_wallet_req.phrase;
    // let salt = "";
    // let wallet_name = "api_wallet";
    // let account_name = "ccccc";
    // let is_default_name = true;
    // let invite_code = None;
    // let api_wallet_type = wallet_database::entities::api_wallet::ApiWalletType::SubAccount;
    // let wallet = wallet_manager
    //     .create_api_wallet(
    //         language_code,
    //         phrase,
    //         salt,
    //         wallet_name,
    //         account_name,
    //         is_default_name,
    //         wallet_password,
    //         invite_code,
    //         api_wallet_type,
    //     )
    //     .await
    //     .result;
    // tracing::warn!("wallet ------------------------ 1: {wallet:#?}");

    // let order_list = wallet_manager.get_api_collect_order_list().await.result;
    // tracing::info!("order_list ------------------- 2: {order_list:#?}");

    // let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";

    // let from = "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt";
    // // let from = "TRLJd4avtuGfW5KZHzigxVxZfVdrwvkoJ5";
    // // let to = "TRLJd4avtuGfW5KZHzigxVxZfVdrwvkoJ5";
    // let to = "TBQSs8KG82iQnLUZj5nygJzSUwwhQJcxHF";
    // // let to = "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt";
    // let value = "20";
    // let trade_no = "0x000000001";
    // let chain_code = "tron";
    // let symbol = "USDT";
    // let token_address = Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string());

    // let res1 = wallet_manager
    //     .api_collect_order(from, to, value, chain_code, token_address, symbol, trade_no, 1, uid)
    //     .await;
    // tracing::info!("api_withdrawal_order ------------------- 4: {res1:#?}");

    // let index = Some(7);
    // let index = None;
    // let address_allock_type = wallet_api::AddressAllockType::ChaBatch;
    // let chain_code = "tron";

    // let res1 = wallet_manager.expand_address(address_allock_type, chain_code, index, uid).await;
    // tracing::info!("expand_address ------------------- 5: {res1:#?}");

    // let wallet = wallet_manager.create_wallet(test_params.create_wallet_req).await?;
    // tracing::warn!("wallet: {wallet:#?}");

    // subscribe(&wallet_manager).await;

    // let sync_res =
    //     wallet_manager.sync_assets_by_wallet(wallet.address.to_string(), None, vec![]).await;
    // tracing::info!("sync res: {sync_res:#?}");
    // let wallet = wallet.unwrap();
    // test_params.create_account_req.wallet_address = wallet.address.clone();

    // let config = wallet_manager.get_config().await;
    // tracing::info!("config result: {config:#?}");
    // let res = wallet_utils::serde_func::serde_to_string(&config)?;
    // tracing::info!("config result: {res}");
    // subscribe(&wallet_manager).await;

    let manager_c = std::sync::Arc::new(wallet_manager.clone());
    test_balance(manager_c).await;

    // if !wallet_manager.sync_api_chains().await?.is_empty() {
    //     wallet_manager.sync_api_wallet_chain_data().await?;
    // }

    loop {
        tokio::select! {
            msg = rx.next() => {
                let data = serde_json::to_string(&msg).unwrap();
                tracing::info!("前端收到数据: {data:?}");
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

#[allow(dead_code)]
async fn test_balance(wallet_manager: Arc<wallet_api::manager::WalletManager>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            // let usdt = wallet_api::infrastructure::asset_calc::get_total_usdt().await;
            // wallet_api::infrastructure::asset_calc::get_price_cache().await;
            // let page = wallet_api::infrastructure::asset_calc::get_asset_snapshot_page(0, 10).await;
            // tracing::info!("usdt: {usdt:#?}");
            // tracing::info!("page: {page:#?}");

            // let res = wallet_api::infrastructure::asset_calc::get_wallet_balance_list().await;

            // let res =
            //     wallet_api::domain::api_wallet::wallet::ApiWalletDomain::get_api_wallet_list()
            //         .await;

            // tracing::info!("get_wallet_balance_list: {res:#?}");

            // let res = wallet_manager
            //     .get_api_assets_list("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166", Some(1), None)
            //     .await;

            // tracing::info!("get_api_assets_list: {res:#?}");
            let res = wallet_manager
                .get_assets_list(
                    "0x0016299F654BF3FaAcCb02E2B4dbbB971a597304",
                    Some(1),
                    None,
                    None,
                    false,
                )
                .await;

            tracing::info!("get_assets_list: {res:#?}");

            let res = wallet_manager
                .get_api_assets(
                    "TLXdEp1kaVx4ePKpZmXqaU8hBnxsvYUoxf",
                    // Some(1),
                    None,
                    "tron",
                    Some("TNDSHKGBmgRx9mDYA9CnxPx55nu672yQw2".to_string()),
                )
                .await;

            tracing::info!("get_api_assets: {res:#?}");

            // let balance_list = wallet_manager
            //     .list_api_wallet_account(
            //         "0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166",
            //         // Some(1),
            //         None,
            //         // Some("tron".to_string()),
            //         Some("tron".to_string()),
            //         // None,
            //         0,
            //         20,
            //     )
            //     .await
            //     .unwrap();
            // let balance_list = serde_json::to_string(&balance_list).unwrap();
            // tracing::info!("list_api_wallet_account balance_list: {balance_list:#?}");

            // let res = wallet_manager
            //     .get_api_account_assets(
            //         1,
            //         "0x234bb8664b5a38573Be7116C10c41cd5c7CbcCD9",
            //         Some("tron".to_string()),
            //     )
            //     .await;
            // tracing::info!("list_api_wallet_account get_api_account_assets: {res:#?}");

            // let res = wallet_manager
            //     .get_api_account_assets(1, "0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166", None)
            //     .await;
            // tracing::info!("get_api_account_assets: {res:#?}");

            // let res =
            //     wallet_api::domain::api_wallet::assets::ApiAssetsDomain::get_api_wallet_assets(
            //         Some("0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166"),
            //         None,
            //         None,
            //     )
            //     .await
            //     .unwrap();
            // tracing::info!("get_api_wallet_assets: {res:#?}");
        }
    });
}

#[allow(dead_code)]
async fn subscribe(wallet_manager: &wallet_api::manager::WalletManager) {
    let topics = vec![
        "wallet/token/eth/usdc".to_string(),
        "wallet/token/tron/trx".to_string(),
        "wallet/token/doge/doge".to_string(),
        "wallet/token/tron/sun".to_string(),
        "wallet/token/tron/win".to_string(),
        "wallet/token/eth/hkby".to_string(),
        "wallet/token/btc/btc".to_string(),
        "wallet/token/eth/eth".to_string(),
        "wallet/token/bnb/bnb".to_string(),
        "wallet/token/sol/sol".to_string(),
        "wallet/token/ltc/ltc".to_string(),
        "wallet/token/eth/link".to_string(),
        "wallet/token/ton/ton".to_string(),
        "wallet/token/sui/sui".to_string(),
        "wallet/token/eth/cake".to_string(),
        "wallet/token/sol/usdt".to_string(),
    ];
    {
        let _ = wallet_manager.mqtt_subscribe(topics, None).await;
    }
}
