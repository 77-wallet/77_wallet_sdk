use sqlx::encode::IsNull::No;
use tokio_stream::StreamExt as _;
use wallet_api::{FrontendNotifyEvent, test::env::get_manager};
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_database::entities::task_queue::KnownTaskName::ApiWithdraw;
// TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wallet_utils::init_test_log();
    let (wallet_manager, test_params) = get_manager().await?;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<FrontendNotifyEvent>();
    let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    wallet_manager.set_frontend_notify_sender(tx).await?;

    if wallet_manager.init(test_params.device_req).await.code != 200 {
        tracing::error!("init failed");
        return Ok(());
    };

    let res = wallet_manager
        .set_invite_code(Some("I1912683353004912640".to_string()))
        .await;
    let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
    tracing::info!("set_invite_code ------------------------0: {res:?}");

    // 创建钱包
    let language_code = 1;
    let phrase = &test_params.create_wallet_req.phrase;
    let salt = "q1111111";
    let wallet_name = "api_wallet";
    let account_name = "ccccc";
    let is_default_name = true;
    let wallet_password = "q1111111";
    let invite_code = None;
    // let api_wallet_type = ApiWalletType::SubAccount;
    let api_wallet_type = ApiWalletType::Withdrawal;
    let wallet = wallet_manager
        .create_api_wallet(language_code, phrase, salt, wallet_name, account_name, is_default_name, wallet_password, invite_code, api_wallet_type)
        .await
        .result;
    tracing::warn!("wallet ------------------------ 1: {wallet:#?}");


    // 获取订单记录
    let order_list = wallet_manager.get_api_withdraw_order_list().await.result;
    tracing::info!("order_list ------------------- 2: {order_list:#?}");


    // 绑定钱包
    // let key = "app_id";
    // let merchain_id = "test_merchain";
    let uid = "04de3a5eff89883fecd1469fbc7621f37122c83d6680b95ad5c67cd9a141cd4e";
    //
    // let res = wallet_manager.bind_merchant(key, merchain_id, uid).await;
    // tracing::info!("res --------------------- 3: {res:?}");


    // ltc
    // let from = "ltc1q943fux4qwp6engz7eds8sp5kk8ru7040rezrq7";
    // let to = "LS5yMXTDu8VD27JHZertA8yYV6PViyJq7F";

    let from = "ltc1q943fux4qwp6engz7eds8sp5kk8ru7040rezrq7";
    let to = "LS5yMXTDu8VD27JHZertA8yYV6PViyJq7F";
    let value = "0.000001";
    let trade_no = "0x0000000040";
    let res1 =  wallet_manager.api_withdrawal_order(from, to, value, "ltc", None, "LTC", trade_no, 1, uid).await;
    tracing::info!("api_withdrawal_order ------------------- 4: {res1:#?}");

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
    while let Some(_data) = rx.next().await {
        // tracing::info!("data: {_data:?}");
    }
    Ok(())
}
