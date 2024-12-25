// use std::{
//     env,
//     path::{Path, PathBuf},
// };
// use anyhow::Result;
// use tracing::info;

// use crate::WalletManager;

// pub struct TestData {
//     pub wallet_manager: WalletManager,
//     pub wallet_env: TestWalletEnv,
// }

// pub struct TestWalletEnv {
//     // pub(crate) storage_dir: PathBuf,
//     pub language_code: u8,
//     pub phrase: String,
//     pub salt: String,
//     pub wallet_name: String,
//     pub password: String,
// }

// impl TestWalletEnv {
//     fn new(
//         language_code: u8,
//         phrase: String,
//         salt: String,
//         wallet_name: String,
//         password: String,
//     ) -> TestWalletEnv {
//         Self {
//             language_code,
//             phrase,
//             salt,
//             wallet_name,
//             password,
//         }
//     }
// }

// pub(crate) struct TestAccountEnv {
//     // pub(crate) storage_dir: PathBuf,
//     pub(crate) derivation_path: String,
//     pub(crate) wallet_name: String,
//     pub(crate) root_password: String,
//     pub(crate) derive_password: String,
// }

// pub(crate) struct TestChainEnv {
//     // pub(crate) storage_dir: PathBuf,
//     pub(crate) name: String,
//     pub(crate) address: String,
//     pub(crate) chain_code: String,
//     pub(crate) status: Option<u8>,
//     pub(crate) rpc_url: String,
//     pub(crate) ws_url: String,
// }

// pub(crate) struct TestCoinEnv {
//     // pub(crate) storage_dir: PathBuf,
//     pub(crate) name: String,
//     pub(crate) address: String,
//     pub(crate) chain_code: String,
//     pub(crate) status: Option<u8>,
//     pub(crate) total_assets: wallet_types::Decimal,
//     pub(crate) total_receipt: wallet_types::Decimal,
//     pub(crate) total_turn_out: wallet_types::Decimal,
// }

// async fn setup_some_test_environment() -> Result<Vec<TestData>> {
//     let test_data = vec![
//         setup_test_environment(Some("钱包A".to_string()), None, false).await?,
//         setup_test_environment(Some("钱包B".to_string()), None, false).await?,
//         setup_test_environment(Some("钱包C".to_string()), None, false).await?,
//     ];
//     Ok(test_data)
// }

// pub async fn setup_test_environment(
//     mut wallet_name: Option<String>,
//     client_id: Option<String>,
//     temp: bool,
// ) -> Result<TestData> {
//     let phrase =
//     // "shaft love depth mercy defy cargo strong control eye machine night test".to_string();
//     // "farm ignore place wing toe attack use turn limb atom grief unlock".to_string();
//     "chuckle practice chicken permit swarm giant improve absurd melt kitchen oppose scrub".to_string();

//     let client_id = client_id.unwrap_or_else(|| "test_data".to_string());
//     // 获取项目根目录
//     let storage_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(&client_id);

//     // 创建测试目录
//     if !storage_dir.exists() {
//         std::fs::create_dir_all(&storage_dir)?;
//     }

//     // 测试参数
//     let language_code = 1;

//     let salt = "".to_string();
//     if temp {
//         info!("storage_dir: {:?}", storage_dir);
//         // 创建临时目录结构
//         let temm_dir = tempfile::tempdir_in(&storage_dir)?;
//         wallet_name = temm_dir
//             .path()
//             .file_name()
//             .map(|name| name.to_string_lossy().to_string());
//     }
//     let wallet_name = wallet_name.unwrap_or_else(|| "example_wallet2".to_string());

//     let password = "example_password".to_string();
//     info!("[setup_test_environment] storage_dir: {:?}", storage_dir);
//     let wallet_manager = WalletManager::new(&storage_dir.to_string_lossy()).await?;
//     let wallet_env = TestWalletEnv {
//         // storage_dir,
//         language_code,
//         phrase,
//         salt,
//         wallet_name: wallet_name.clone(),
//         password,
//     };

//     // let derivation_path = "m/44'/60'/0'/0/1".to_string();
//     // let account_env = TestAccountEnv {
//     //     derivation_path,
//     //     wallet_name,
//     //     root_password: todo!(),
//     //     derive_password: todo!(),
//     // };

//     Ok(TestData {
//         wallet_manager,
//         wallet_env,
//     })
// }

// pub(crate) fn print_dir_structure(dir: &Path, level: usize) {
//     if let Ok(entries) = std::fs::read_dir(dir) {
//         for entry in entries.flatten() {
//             let path = entry.path();
//             for _ in 0..level {
//                 print!("  ");
//             }
//             if path.is_dir() {
//                 info!("{}/", path.file_name().unwrap().to_string_lossy());
//                 print_dir_structure(&path, level + 1);
//             } else {
//                 info!("{}", path.file_name().unwrap().to_string_lossy());
//             }
//         }
//     }
// }

use anyhow::Result;
use std::{env, path::PathBuf};
use tracing::info;

use crate::WalletManager;

pub struct TestData {
    pub wallet_manager: WalletManager,
    pub wallet_env: TestWalletEnv,
}

pub struct TestWalletEnv {
    // pub(crate) storage_dir: PathBuf,
    pub language_code: u8,
    pub phrase: String,
    pub salt: String,
    pub wallet_name: String,
    pub password: String,
}

impl TestWalletEnv {
    fn new(
        language_code: u8,
        phrase: String,
        salt: String,
        wallet_name: String,
        password: String,
    ) -> TestWalletEnv {
        Self {
            language_code,
            phrase,
            salt,
            wallet_name,
            password,
        }
    }
}

pub async fn setup_test_environment(
    mut wallet_name: Option<String>,
    client_id: Option<String>,
    temp: bool,
    phrase: Option<String>,
) -> Result<TestData> {
    let phrase = phrase.unwrap_or_else(|| {
        "chuckle practice chicken permit swarm giant improve absurd melt kitchen oppose scrub"
            .to_string()
    });

    // "shaft love depth mercy defy cargo strong control eye machine night test".to_string();
    // "farm ignore place wing toe attack use turn limb atom grief unlock".to_string();
    // "chuckle practice chicken permit swarm giant improve absurd melt kitchen oppose scrub".to_string();

    let client_id = client_id.unwrap_or_else(|| "test_data".to_string());
    // 获取项目根目录
    let storage_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join(&client_id);

    // 创建测试目录
    if !storage_dir.exists() {
        std::fs::create_dir_all(&storage_dir)?;
    }

    // 测试参数
    let language_code = 1;
    // let salt = "12345678".to_string();
    // let salt = "1234qwer".to_string();
    // let salt = "12345qwe".to_string();
    let salt = "qwer1234".to_string();
    // let salt = "123".to_string();
    // let salt = "".to_string();
    if temp {
        info!("storage_dir: {:?}", storage_dir);
        // 创建临时目录结构
        let temm_dir = tempfile::tempdir_in(&storage_dir)?;
        wallet_name = temm_dir
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().to_string());
    }
    let wallet_name = wallet_name.unwrap_or_else(|| "example_wallet".to_string());

    let password = "123456".to_string();
    info!("[setup_test_environment] storage_dir: {:?}", storage_dir);
    let wallet_manager = WalletManager::new(
        "bdb6412a9cb4b12c48ebe1ef4e9f052b07af519b7485cd38a95f38d89df97cb8",
        "ANDROID",
        &storage_dir.to_string_lossy(),
        None,
    )
    .await?;

    let wallet_env = TestWalletEnv::new(language_code, phrase, salt, wallet_name, password);

    // let derivation_path = "m/44'/60'/0'/0/1".to_string();
    // let account_env = TestAccountEnv {
    //     derivation_path,
    //     wallet_name,
    //     root_password: todo!(),
    //     derive_password: todo!(),
    // };

    Ok(TestData {
        wallet_manager,
        wallet_env,
    })
}
