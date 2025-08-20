use wallet_crypto::{EncryptedJsonGenerator as _, KeystoreJsonGenerator};
use wallet_database::{
    entities::{api_account::CreateApiAccountVo, api_wallet::ApiWalletType},
    repositories::{
        ResourcesRepo, api_account::ApiAccountRepo, api_wallet::ApiWalletRepo,
        device::DeviceRepoTrait as _,
    },
};
use wallet_transport_backend::request::AddressInitReq;
use wallet_types::chain::address::r#type::AddressType;

use crate::{domain::app::config::ConfigDomain, response_vo::account::CreateAccountRes};

pub(crate) struct ApiAccountDomain {}

impl ApiAccountDomain {
    pub(crate) async fn create_api_account(
        repo: &mut ResourcesRepo,
        seed: &[u8],
        instance: &wallet_chain_instance::instance::ChainObject,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(CreateAccountRes, String, Option<AddressInitReq>), crate::ServiceError> {
        let (address, derivation_path, address_init_req) = Self::derive_subkey(
            repo,
            uid,
            seed,
            wallet_address,
            account_index_map,
            instance,
            name,
            is_default_name,
            wallet_password,
            api_wallet_type,
        )
        .await?;
        let res = CreateAccountRes {
            address: address.to_string(),
        };
        // let task_data = Self::address_init(
        //     repo,
        //     uid,
        //     &address,
        //     account_index_map.input_index,
        //     &instance.chain_code().to_string(),
        //     &name,
        // )
        // .await?;

        Ok((res, derivation_path, address_init_req))
    }

    // pub(crate) async fn decrypt_seed(
    //     password: &str,
    //     seed: &str,
    // ) -> Result<String, crate::ServiceError> {
    //     let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), seed)?;
    //     let seed = wallet_utils::conversion::vec_to_string(&data)?;

    //     Ok(seed)
    // }

    // pub(crate) async fn decrypt_phrase(
    //     password: &str,
    //     phrase: &str,
    // ) -> Result<String, crate::ServiceError> {
    //     let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), phrase)?;
    //     let phrase = wallet_utils::conversion::vec_to_string(&data)?;
    //     Ok(phrase)
    // }

    pub(crate) async fn derive_subkey(
        repo: &mut ResourcesRepo,
        uid: &str,
        seed: &[u8],
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        instance: &wallet_chain_instance::instance::ChainObject,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(String, String, Option<AddressInitReq>), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let account_name = if is_default_name {
            format!("{account_name}{}", account_index_map.account_id)
        } else {
            account_name.to_string()
        };

        let keypair = instance
            .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        let derivation_path = keypair.derivation_path();
        let chain_code = keypair.chain_code().to_string();

        let address_type = instance.address_type();
        let address = keypair.address();
        let pubkey = keypair.pubkey();
        let private_key = keypair.private_key()?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let rng = rand::thread_rng();
        let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        let private_key = wallet_utils::serde_func::serde_to_vec(&private_key)?;
        let private_key = generator.generate(wallet_password.as_bytes(), &private_key)?;
        let private_key = wallet_utils::serde_func::serde_to_string(&private_key)?;

        let mut req = CreateApiAccountVo::new(
            account_index_map.account_id,
            &address,
            &pubkey,
            &private_key,
            wallet_address,
            &derivation_path,
            &account_index_map.input_index.to_string(),
            &chain_code,
            &account_name,
            api_wallet_type,
        );

        let Some(device) = repo.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        let account = ApiAccountRepo::find_one(
            &pool,
            &address,
            &chain_code,
            &address_type.to_string(),
            api_wallet_type,
        )
        .await?;

        let address_init_req = if let Some(account) = account {
            tracing::info!("已存在: {}", account.address);
            None
        } else {
            Some(wallet_transport_backend::request::AddressInitReq::new(
                uid,
                &address,
                account_index_map.input_index,
                &instance.chain_code().to_string(),
                &device.sn,
                vec!["".to_string()],
                &account_name,
            ))
        };

        match address_type {
            AddressType::Btc(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Ltc(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Dog(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            AddressType::Ton(address_type) => {
                req = req.with_address_type(address_type.as_ref());
            }
            _ => {}
        }

        ApiAccountRepo::upsert(&pool, vec![req]).await?;

        Ok((address, derivation_path, address_init_req))
    }

    pub(crate) async fn address_used(
        chain_code: &str,
        index: i32,
        uid: &str,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid, api_wallet_type)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::NotFound,
            ))?;
        let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;

        let account = ApiAccountRepo::find_one_by_wallet_address_index(
            &pool,
            &api_wallet.address,
            chain_code,
            index.account_id,
        )
        .await?;
        if let Some(account) = account {
            ApiAccountRepo::mark_as_used(&pool, &api_wallet.address, account.account_id).await?;
        }

        Ok(())
    }
}
