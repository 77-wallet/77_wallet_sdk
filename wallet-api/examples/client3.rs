use sqlx::encode::IsNull::No;
use tokio_stream::StreamExt as _;
use wallet_api::{
    manager::WalletManager,
    messaging::notify::FrontendNotifyEvent,
    request::api_wallet::account::CreateApiAccountReq,
    test::env::{TestParams, get_manager},
    xlog::init_log,
};
use wallet_database::entities::{api_wallet::ApiWalletType, api_withdraw::ApiWithdrawStatus};
use wallet_transport_backend::request::api_wallet::strategy::{ChainConfig, IndexAndAddress};
use wallet_types::chain::chain::ChainCode;

async fn run_collect_strategy(
    wallet_manager: &WalletManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let wallet_uid = "fd73defade2a1041cec77825f5dc520db3748fea7b48d572136e86e1cf57f30e";

    let res = wallet_manager.get_collect_strategy(wallet_uid).await?;
    tracing::info!("get collect strategy -------------------- {:?}", res);

    let res = wallet_manager
        .update_collect_strategy(
            &wallet_uid,
            80,
            vec![ChainConfig {
                chain_code: ChainCode::Tron.to_string(),
                chain_address_type: Some("Tron".to_string()),
                normal_address: IndexAndAddress {
                    index: Some(0),
                    address: "TDpBeopE7JD7sZQjXnJzQ4RqdopeQYB9nf".to_string(),
                },
                risk_address: IndexAndAddress {
                    index: Some(1),
                    address: "TDpBeopE7JD7sZQjXnJzQ4RqdopeQYB9nf".to_string(),
                },
            }],
        )
        .await;
    match res {
        Ok(reason) => {
            tracing::info!("更新归集策略成功 --------------------- ");
        }
        Err(err) => {
            tracing::error!("更新归集策略失败 --------------------- 5: {err:#?}");
        }
    }
    Ok(())
}

async fn run_withdrawal_strategy(
    wallet_manager: &WalletManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let wallet_uid = "78da2f53e3ee6c651859b557a1c74067d6c44db356f0b2835c09c03f8541f78a";
    let res = wallet_manager.get_withdrawal_strategy(wallet_uid).await?;
    tracing::info!("get withdrawal strategy -------------------- {:?}", res);
    let res = wallet_manager
        .update_withdrawal_strategy(
            &wallet_uid,
            5,
            vec![ChainConfig {
                chain_code: ChainCode::Tron.to_string(),
                chain_address_type: Some("Tron".to_string()),
                normal_address: IndexAndAddress {
                    index: Some(0),
                    address: "TCLQRc8WFeY5rWHfNZr7PUNhC9pyjbnEEo".to_string(),
                },
                risk_address: IndexAndAddress {
                    index: Some(1),
                    address: "TR938TdXsPT18Mkt3yJon8DFbhUwdusgRE".to_string(),
                },
            }],
        )
        .await;
    match res {
        Ok(reason) => {
            tracing::info!("更新提币策略成功 --------------------- ");
        }
        Err(err) => {
            tracing::error!("更新提币策略失败 --------------------- 5: {err:#?}");
        }
    }
    Ok(())
}

async fn run_withdraw_order(
    wallet_manager: &WalletManager,
) -> Result<(), Box<dyn std::error::Error>> {
    // let trade_no = "W1979025709485711360";
    // wallet_manager.reject_api_withdrawal_order(trade_no).await?;
    // tracing::info!("拒绝提币策略成功 --------------------- ");

    let uid = "78da2f53e3ee6c651859b557a1c74067d6c44db356f0b2835c09c03f8541f78a";
    let res = wallet_manager
        .page_api_withdraw_order(uid, vec![ApiWithdrawStatus::AuditReject as u8, ApiWithdrawStatus::SendingTxFailed as u8], 0, 10)
        .await?;
    for e in &res.data {
        let res = serde_json::to_string(e).unwrap();
        tracing::info!("-------- {:?}", res);
    }
    tracing::info!("获取提币拒绝策略数据 --------------------- {:?}", res);
    Ok(())
}

async fn run_tx(wallet_manager: &WalletManager) -> Result<(), Box<dyn std::error::Error>> {
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

async fn run(
    wallet_manager: &WalletManager,
    test_params: &TestParams,
) -> Result<(), Box<dyn std::error::Error>> {
    // 创建钱包
    let language_code = 1;
    let phrase = &test_params.create_wallet_req.phrase;
    let salt = "q1111111";
    let wallet_name = "api_wallet";
    let wallet_password = "q1111111";
    // let binding_address = None;
    // let wallet_uid = wallet_manager
    //     .create_api_wallet(
    //         language_code,
    //         phrase,
    //         salt,
    //         wallet_name,
    //         wallet_password,
    //         None,
    //         ApiWalletType::SubAccount,
    //         binding_address,
    //     )
    //     .await?;
    // tracing::info!("子wallet创建成功 ------------------------ 1: {wallet_uid:#?}");

    let res = wallet_manager.get_api_wallet_list().await?;
    tracing::info!("get withdraw wallet list ------------------------ 2: {res:#?}");

    let salt1 = "q1111112";
    // let binding_address = None;
    // let withdrawal_uid = wallet_manager
    //     .create_api_wallet(
    //         language_code,
    //         phrase,
    //         salt1,
    //         wallet_name,
    //         wallet_password,
    //         None,
    //         ApiWalletType::Withdrawal,
    //         binding_address,
    //     )
    //     .await?;
    // tracing::info!("withdraw wallet 创建成功 ------------------------ 2: {withdrawal_uid:#?}");

    wallet_manager.set_passwd_cache(wallet_password).await?;
    tracing::info!("绑定钱包之前必须设置密码成功 ------------------------ ");

    // let res = wallet_manager
    //     .scan_bind(
    //         "dd3306ba4b334cfe86dade1e46301b0d",
    //         "68be725ea7307e042404e267",
    //         &wallet_uid,
    //         &withdrawal_uid,
    //     )
    //     .await?;
    // tracing::info!("绑定app成功 ------------------- 3: {res:#?}");

    // run_collect_strategy(wallet_manager).await?;
    // run_withdrawal_strategy(wallet_manager).await?;
    run_withdraw_order(wallet_manager).await?;

    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
