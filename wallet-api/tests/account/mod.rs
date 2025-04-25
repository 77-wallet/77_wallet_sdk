use std::{env, path::PathBuf};
use wallet_api::{InitDeviceReq, WalletManager};
use wallet_chain_instance::instance::ChainObject;
use wallet_utils::init_test_log;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();
    let config = wallet_api::Config::new(&wallet_api::test::env::get_config().unwrap()).unwrap();

    WalletManager::new("guangxiang", "ANDROID", &path, None, config)
        .await
        .unwrap()
}

#[tokio::test]
async fn create_device() {
    let manager = get_manager().await;

    let req = InitDeviceReq {
        device_type: "ios".to_string(),
        sn: "xxx12313000899".to_string(),
        code: "aaaccc".to_string(),
        system_ver: "cc".to_string(),
        iemi: None,
        meid: None,
        iccid: None,
        mem: None,
        app_id: None,
        package_id: None,
        app_version: "1.2.3".to_string(),
        // invitee: false,
    };
    let res = manager.init_device(req).await;
    tracing::info!("response {:?}", res);
}

#[tokio::test]
async fn create_wallet() {
    let wallet_manager = get_manager().await;
    let phrase =
        "february crunch banner cave afford chuckle left plate session tackle crash approve";
    let salt = "qwer1234";
    let wallet_name = "my_wallet";
    let account_name = "账户";
    let password = "123456";
    let req = wallet_api::CreateWalletReq::new(
        1,
        phrase,
        salt,
        wallet_name,
        account_name,
        true,
        password,
        None,
        None,
    );
    let res = wallet_manager.create_wallet(req).await;

    println!("创建的钱包{:?}", res);
}

#[tokio::test]
async fn create_account() {
    let wallet_manager = get_manager().await;
    // let wallet_name = "0x3d669d78532F763118561b55daa431956ede4155";
    let wallet_name = "0x85dc10e7E48e0Aa4368Ed91707297F6a63D68dA9";
    let account_name = "账户";
    let root_password = "123456";
    let req = wallet_api::CreateAccountReq::new(
        wallet_name,
        root_password,
        None,
        None,
        Some(1),
        account_name,
        false,
    );

    let resp = wallet_manager.create_account(req).await;
    tracing::info!("create_account {:?}", resp);

    // for _i in 0..2 {
    //     let resp = wallet_manager.create_account(req.clone()).await;
    //     tracing::info!("create_account {:?}", resp);
    // }
}

#[tokio::test]
async fn sync_assets() {
    let wallet_manager = get_manager().await;
    let _c = wallet_manager.sync_assets(vec![], None, vec![]).await;
}

#[tokio::test]
async fn sync_assets_by_wallet() {
    let wallet_manager = get_manager().await;
    let wallet_address = "0xAE7fEc45b1a63c2870B80F2496870bd064C1C937".to_string();
    let account_id = None;
    let symbol = vec!["TRX".to_string()];

    let _c = wallet_manager
        .sync_assets_by_wallet(wallet_address, account_id, symbol)
        .await;
}

#[tokio::test]
async fn test_generate_phrase() {
    let wallet_manager = get_manager().await;
    let c = wallet_manager.generate_phrase(1, 12);

    tracing::info!("response {:?}", c)
}

#[tokio::test]
async fn test_show_key() {
    init_test_log();

    let parse = "member diesel marine culture boat differ spirit patient drum fix chunk sadness"
        .to_string();
    let (_key, seed) = wallet_core::xpriv::generate_master_key(1, &parse, "1234qwer").unwrap();

    let chain_code = "sol";
    let network = wallet_types::chain::network::NetworkKind::Mainnet;

    let address_type = Some("p2wpkh".to_string());
    let object = ChainObject::new(chain_code, address_type, network).unwrap();

    let keypair = object
        .gen_keypair_with_index_address_type(&seed, 10)
        .unwrap();

    tracing::info!("address = {}", keypair.address());
    tracing::info!("key = {}", keypair.private_key().unwrap());
}

#[tokio::test]
async fn test_delete_account() {
    let wallet_manager = get_manager().await;

    let wallet_address = "0x655128b428d294CCEa874a2B05aE090055C89b59";
    let account_id = 1;

    let c = wallet_manager
        .physical_delete_account(wallet_address, account_id, "123456")
        .await;

    tracing::info!("response {:?}", c)
}
