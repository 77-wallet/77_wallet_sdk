use crate::{
    domain::app::config::ConfigDomain, error::service::ServiceError,
    response_vo::account::CreateAccountRes,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::{
        api_account::{ApiAccountEntity, CreateApiAccountVo},
        api_wallet::ApiWalletType,
        chain::ChainEntity,
    },
    repositories::{api_account::ApiAccountRepo, api_wallet::ApiWalletRepo, device::DeviceRepo},
};
use wallet_transport_backend::request::AddressInitReq;
use wallet_types::chain::{address::r#type::AddressType, chain::ChainCode};

pub(crate) struct ApiAccountDomain {}

impl ApiAccountDomain {
    pub(crate) async fn create_api_account(
        seed: &[u8],
        instance: &wallet_chain_instance::instance::ChainObject,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(CreateAccountRes, Option<AddressInitReq>), crate::error::service::ServiceError>
    {
        let (address, address_init_req) = Self::derive_subkey(
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
        let res = CreateAccountRes { address: address.to_string() };
        // let task_data = Self::address_init(
        //     repo,
        //     uid,
        //     &address,
        //     account_index_map.input_index,
        //     &instance.chain_code().to_string(),
        //     &name,
        // )
        // .await?;

        Ok((res, address_init_req))
    }

    pub(crate) async fn get_private_key(
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or(crate::error::business::BusinessError::Account(
                crate::error::business::account::AccountError::NotFound(address.to_string()),
            ))?;

        let key = KeystoreJsonDecryptor.decrypt(password.as_ref(), &account.private_key)?;

        tracing::info!("get_private_key ------------------- 6: {chain_code}");
        let chain = ChainEntity::chain_node_info(pool.as_ref(), chain_code).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::NotFound(chain_code.to_string()),
                ),
            ),
        )?;

        tracing::info!("chain_code ------------------- 7: {chain_code}");
        let chain_code: ChainCode = chain_code.try_into()?;
        let private_key = match chain_code {
            ChainCode::Solana => {
                wallet_utils::parse_func::sol_keypair_from_bytes(&key)?.to_base58_string()
            }
            ChainCode::Bitcoin => {
                wallet_chain_interact::btc::wif_private_key(&key, chain.network.as_str().into())?
            }
            ChainCode::Dogcoin => {
                wallet_chain_interact::dog::wif_private_key(&key, chain.network.as_str().into())?
            }
            ChainCode::Litecoin => {
                wallet_chain_interact::ltc::wif_private_key(&key, chain.network.as_str().into())?
            }
            _ => hex::encode(key),
        };
        Ok(private_key.into())
    }

    // pub(crate) async fn decrypt_phrase(
    //     password: &str,
    //     phrase: &str,
    // ) -> Result<String, crate::ServiceError> {
    //     let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), phrase)?;
    //     let phrase = wallet_utils::conversion::vec_to_string(&data)?;
    //     Ok(phrase)
    // }

    pub(crate) async fn derive_subkey(
        uid: &str,
        seed: &[u8],
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        instance: &wallet_chain_instance::instance::ChainObject,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(String, Option<AddressInitReq>), crate::error::service::ServiceError> {
        let account_name = if is_default_name {
            format!("{account_name}{}", account_index_map.account_id)
        } else {
            account_name.to_string()
        };

        let (address, pubkey, private_key, chain_code, derivation_path) = {
            let keypair = instance
                .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
                .map_err(|e| crate::error::system::SystemError::Service(e.to_string()))?;
            (
                keypair.address(),
                keypair.pubkey(),
                keypair.private_key_bytes()?,
                keypair.chain_code().to_string(),
                keypair.derivation_path(),
            )
        };

        // let keypair = instance
        //     .gen_keypair_with_index_address_type(seed, account_index_map.input_index)
        //     .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        // let derivation_path = keypair.derivation_path();
        // let chain_code = keypair.chain_code().to_string();

        let address_type = instance.address_type();
        // let address = keypair.address();
        // let pubkey = keypair.pubkey();
        // let private_key = keypair.private_key()?;
        // let private_key = keypair.private_key_bytes()?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = wallet_database::repositories::api_account::find_one(
            pool.clone(),
            &address,
            &chain_code,
            &address_type.to_string(),
            api_wallet_type,
        )
        .await?;
        let Some(device) = DeviceRepo::get_device_info(pool.clone()).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        // let private_key = wallet_utils::serde_func::serde_to_vec(&private_key)?;

        let private_key = {
            let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
            let rng = rand::thread_rng();
            let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
            generator.generate(wallet_password.as_bytes(), &private_key)?
        };

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

        Ok((address, address_init_req))
    }

    pub(crate) async fn address_used(
        chain_code: &str,
        index: i32,
        uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;

        let accounts = ApiAccountRepo::find_all_by_wallet_address_index(
            &pool,
            &api_wallet.address,
            chain_code,
            index.account_id,
        )
        .await?;
        for account in accounts {
            ApiAccountRepo::mark_as_used(
                &pool,
                &api_wallet.address,
                account.account_id,
                chain_code,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn account(
        chain_code: &str,
        address: &str,
    ) -> Result<ApiAccountEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or(crate::error::business::BusinessError::Account(
                crate::error::business::account::AccountError::NotFound(address.to_string()),
            ))?;
        Ok(account)
    }

    pub fn next_account_indices(mut existing: Vec<u32>, count: usize) -> Vec<u32> {
        existing.sort();
        let set: std::collections::HashSet<u32> = existing.into_iter().collect();

        let mut result = Vec::new();
        let mut candidate = 1;

        while result.len() < count {
            if !set.contains(&candidate) {
                result.push(candidate);
            }
            candidate += 1;
        }

        result
    }

    // pub async fn get_addresses(
    //     &self,
    //     repo: &mut ResourcesRepo,
    //     address: &str,
    //     account_id: Option<u32>,
    //     chain_codes: Vec<String>,
    //     is_multisig: Option<bool>,
    // ) -> Result<Vec<AddressChainCode>, ServiceError> {
    //     let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    //     let mut account_addresses = Vec::new();

    //     // let chain_codes = if let Some(chain_code) = &chain_code {
    //     //     vec![chain_code.to_string()]
    //     // } else {
    //     //     vec![]
    //     // };

    //     if let Some(is_multisig) = is_multisig {
    //         if is_multisig {
    //             tracing::debug!("开始查询多签账户 is_multisig: {is_multisig}");
    //             tracing::debug!("多签账户地址 address: {address}");

    //             // 查询多签账户下的资产
    //             let account =
    //                 super::multisig::MultisigDomain::account_by_address(address, true, &pool)
    //                     .await?;
    //             tracing::debug!("查询成功 account: {account:?}");
    //             account_addresses.push(AddressChainCode {
    //                 address: account.address,
    //                 chain_code: account.chain_code,
    //             });
    //         } else {
    //             // 获取钱包下的这个账户的所有地址
    //             let accounts = ApiAccountRepo:: api_account_list(
    //                 &pool,
    //                     Some(address),
    //                     account_id,
    //                     chain_codes,
    //                 )
    //                 .await?;

    //             for account in accounts {
    //                 if !account_addresses.iter().any(|address| {
    //                     address.address == account.address
    //                         && address.chain_code == account.chain_code
    //                 }) {
    //                     account_addresses.push(AddressChainCode {
    //                         address: account.address,
    //                         chain_code: account.chain_code,
    //                     });
    //                 }
    //             }
    //         }
    //     } else {
    //         // 获取钱包下的这个账户的所有地址
    //         let accounts = repo
    //             .account_list_by_wallet_address_and_account_id_and_chain_codes(
    //                 Some(address),
    //                 account_id,
    //                 chain_codes,
    //             )
    //             .await?;
    //         for account in accounts {
    //             if !account_addresses.iter().any(|address| {
    //                 address.address == account.address && address.chain_code == account.chain_code
    //             }) {
    //                 account_addresses.push(AddressChainCode {
    //                     address: account.address,
    //                     chain_code: account.chain_code,
    //                 });
    //             }
    //         }
    //     }
    //     tracing::debug!("[get addresses] account_addresses: {account_addresses:?}");
    //     Ok(account_addresses)
    // }
}

#[cfg(test)]
mod test {
    use alloy::signers::local::PrivateKeySigner;
    use wallet_crypto::{EncryptedJsonDecryptor, KeystoreJsonDecryptor};

    async fn test_keystore_key() -> Result<(), Box<dyn std::error::Error>> {
        let key = KeystoreJsonDecryptor.decrypt("q1111111".as_bytes(),r#"{"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"cafaaf94330ae23b8a8eb64660d42740"},"ciphertext":"19e4fee3686f858bc45946665ee751a9964ef956d06ecee2f7a90021bd946529","kdf":"argon2id","kdfparams":{"dklen":32,"time_cost":5,"memory_cost":131072,"parallelism":8,"salt":[63,15,27,159,163,164,60,107,41,155,135,165,52,165,224,219,52,197,122,0,161,45,75,23,49,198,4,140,1,67,182,207]},"mac":"faf334de5be2b30526a8755980372718aad9b477b52753bde820cb6673bba7a9"},"id":"83577d8c-af30-44e6-9f06-5e616b0ac2be","version":3}"#)?;
        let h = hex::encode(key);
        let signer: PrivateKeySigner = h.parse().map_err(|_| {
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            )
        })?;
        Ok(())
    }

    #[tokio::test]
    async fn test_keystore() {
        let res = test_keystore_key().await;
        assert!(res.is_ok());
    }
}
