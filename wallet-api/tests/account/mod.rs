use anyhow::Result;
use std::{env, path::PathBuf};
use wallet_api::{dirs::Dirs, manager::WalletManager};
use wallet_chain_instance::instance::ChainObject;
use wallet_types::chain::{
    address::r#type::{AddressType, TonAddressType},
    chain::ChainCode,
};
use wallet_utils::init_test_log;

async fn get_manager() -> WalletManager {
    init_test_log();
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("test_data")
        .to_string_lossy()
        .to_string();
    let config =
        wallet_api::config::Config::new(&wallet_api::test::env::get_config().unwrap()).unwrap();
    let dirs = Dirs::new(&path).unwrap();

    WalletManager::new("guangxiang", "ANDROID", None, config, dirs).await.unwrap()
}

#[tokio::test]
async fn create_device() {
    let manager = get_manager().await;

    let req = wallet_api::request::devices::InitDeviceReq {
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
    let phrase = "";
    let salt = "";
    let wallet_name = "my_wallet";
    let account_name = "账户";
    let password = "123456";
    let req = wallet_api::request::wallet::CreateWalletReq::new(
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
async fn delete_wallet() {
    let wallet_manager = get_manager().await;

    let addr = "0xEa80C3E12F7AB803f2bf35d79501172D505F2D98";

    let res = wallet_manager.physical_delete_wallet(&addr).await;

    println!("创建的钱包{:?}", res);
}

#[tokio::test]
async fn create_account() {
    let wallet_manager = get_manager().await;
    // let wallet_name = "0x3d669d78532F763118561b55daa431956ede4155";
    let wallet_name = "0x868Bd024461e572555c26Ed196FfabAA475BFcCd";
    let account_name = "账户";
    let root_password = "123456";
    let req = wallet_api::request::account::CreateAccountReq::new(
        wallet_name,
        root_password,
        None,
        None,
        None,
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
async fn physical_delete() {
    let wallet_manager = get_manager().await;
    let wallet_address = "0x7F2A20beC3a5980C8105E642b9cC0FBEd73D3190";
    let account_id = 1;
    let root_password = "123456";

    let resp =
        wallet_manager.physical_delete_account(&wallet_address, account_id, &root_password).await;
    tracing::info!("create_account {:?}", resp);
}

#[tokio::test]
async fn test_generate_phrase() -> Result<()> {
    let wallet_manager = get_manager().await;
    let c = wallet_manager.generate_phrase(1, 12)?;

    let phrase = c.phrases.join(" ");

    tracing::info!("{}", phrase);
    Ok(())
}

#[tokio::test]
async fn test_show_key() {
    init_test_log();

    let parse = "".to_string();
    let (_key, seed) = wallet_core::xpriv::generate_master_key(1, &parse, "").unwrap();

    let chain_code = ChainCode::Solana;
    let network = wallet_types::chain::network::NetworkKind::Mainnet;

    let address_type = AddressType::Ton(TonAddressType::V4R2);

    let object: ChainObject = (&chain_code, &address_type, network).try_into().unwrap();

    let keypair = object.gen_keypair_with_index_address_type(&seed, 0).unwrap();

    tracing::info!("address = {}", keypair.address());
    tracing::info!("key = {}", keypair.private_key().unwrap());
}

#[tokio::test]
async fn test_delete_account() {
    let wallet_manager = get_manager().await;

    let wallet_address = "0x655128b428d294CCEa874a2B05aE090055C89b59";
    let account_id = 1;

    let c = wallet_manager.physical_delete_account(wallet_address, account_id, "123456").await;

    tracing::info!("response {:?}", c)
}

#[tokio::test]
async fn test_current_chain_address() -> Result<()> {
    let wallet_manager = get_manager().await;

    let uid = "f091ca89e48bc1cd3e4cb84e8d3e3d9e2564e3616efd1feb468793687037d66f".to_string();
    let account_id = 1;
    let chain_code = "tron".to_string();

    let c = wallet_manager.current_chain_address(uid, account_id, chain_code).await?;

    tracing::info!("response {}", serde_json::to_string(&c).unwrap());
    Ok(())
}

#[tokio::test]
async fn test_current_address() -> Result<()> {
    let wallet_manager = get_manager().await;

    let wallet_address = "0x32296819E5A42B04cb21f6bA16De3f3C4B024DDc".to_string();
    let account_id = 1;

    let c = wallet_manager.current_account(wallet_address, account_id).await?;

    tracing::info!("response {}", serde_json::to_string(&c).unwrap());
    Ok(())
}
