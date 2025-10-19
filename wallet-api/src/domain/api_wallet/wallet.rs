use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::{api_wallet::ApiWalletType, device::DeviceEntity},
    repositories::{api_wallet::wallet::ApiWalletRepo, wallet::WalletRepo},
};
use wallet_transport_backend::{
    request::api_wallet::wallet::{AppIdImportReq, BindAppIdReq},
    response_vo::api_wallet::wallet::{
        KeysUidCheckRes, QueryUidBindInfoRes, QueryWalletActivationInfoResp,
    },
};

use crate::{
    context::CONTEXT,
    domain::{
        api_wallet::account::ApiAccountDomain,
        app::{DeviceDomain, config::ConfigDomain},
    },
    error::service::ServiceError,
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AddressAllockType,
    response_vo::api_wallet::wallet::{ApiWalletItem, ApiWalletList},
};

pub struct ApiWalletDomain {}

impl ApiWalletDomain {
    pub(crate) async fn upsert_api_wallet(
        uid: &str,
        wallet_name: &str,
        wallet_address: &str,
        password: &str,
        phrase: &str,
        seed: &[u8],
        api_wallet_type: ApiWalletType,
        binding_address: Option<&str>,
    ) -> Result<(), ServiceError> {
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // let phrase = wallet_utils::serde_func::serde_to_vec(&phrase)?;

        // let rng = rand::thread_rng();
        // let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        // let phrase = generator.generate(password.as_bytes(), &phrase)?;
        // let phrase = wallet_utils::serde_func::serde_to_string(&phrase)?;
        // let seed =
        //     KeystoreJsonGenerator::new(rng, algorithm).generate(password.as_bytes(), seed)?;
        // let seed = wallet_utils::serde_func::serde_to_string(&seed)?;

        let (phrase_enc, seed_enc) = {
            // rng 在这个 block 内创建并使用，block 结束时 rng 被 drop
            let rng = rand::thread_rng();

            // 用 rng 生成 phrase
            let mut gen1 = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
            let phrase_keystore = gen1.generate(password.as_bytes(), phrase.as_bytes())?;
            let phrase_enc = wallet_utils::serde_func::serde_to_string(&phrase_keystore)?;

            // 用 rng（或其 clone）生成 seed
            let mut gen2 = KeystoreJsonGenerator::new(rng, algorithm.clone());
            let seed_keystore = gen2.generate(password.as_bytes(), seed)?;
            let seed_enc = wallet_utils::serde_func::serde_to_string(&seed_keystore)?;

            (phrase_enc, seed_enc)
        };

        ApiWalletRepo::upsert(
            &pool,
            &uid,
            wallet_name,
            wallet_address,
            &phrase_enc,
            &seed_enc,
            api_wallet_type,
            binding_address,
        )
        .await?;

        if let Some(binding_address) = binding_address {
            ApiWalletRepo::bind_withdraw_and_subaccount_relation(
                pool,
                binding_address,
                wallet_address,
            )
            .await?;
        }

        Ok(())
    }

    pub(crate) async fn decrypt_seed(password: &str, seed: &str) -> Result<Vec<u8>, ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), seed)?;
        Ok(data)
    }

    pub(crate) async fn decrypt_phrase(
        password: &str,
        phrase: &str,
    ) -> Result<String, ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), phrase)?;
        let data = wallet_utils::conversion::vec_to_string(&data)?;
        Ok(data)
    }

    pub(crate) async fn check_normal_wallet_exist(address: &str) -> Result<bool, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        Ok(WalletRepo::detail(&pool, address).await?.is_some())
    }

    /// 落盘数据：uid绑定数据
    pub(crate) async fn db_save_bind_data(
        recharge_address: &str,
        withdrawal_address: &str,
        org_id: &str,
        app_id: &str,
    ) -> Result<(), ServiceError> {
        ApiWalletDomain::bind_uid(recharge_address, org_id, app_id).await?;
        ApiWalletDomain::bind_uid(withdrawal_address, org_id, app_id).await?;

        // ApiWalletDomain::bind_withdraw_and_subaccount_relation(
        //     recharge_address,
        //     withdrawal_address,
        // )
        // .await?;

        Ok(())
    }

    pub(crate) async fn bind_uid(
        address: &str,
        merchain_id: &str,
        org_app_id: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWalletRepo::update_merchant_id(&pool, &address, merchain_id).await?;
        ApiWalletRepo::update_app_id(&pool, &address, org_app_id).await?;

        Ok(())
    }

    // pub(crate) async fn bind_withdraw_and_subaccount_relation(
    //     subaccount_uid: &str,
    //     withdraw_uid: &str,
    // ) -> Result<(), ServiceError> {
    //     let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

    //     ApiWalletRepo::bind_withdraw_and_subaccount_relation(
    //         pool.clone(),
    //         &subaccount_uid,
    //         &withdraw_uid,
    //     )
    //     .await?;

    //     ApiWalletRepo::bind_withdraw_and_subaccount_relation(pool, &withdraw_uid, &subaccount_uid)
    //         .await?;
    //     Ok(())
    // }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        ApiWalletRepo::upbind_uid(&pool, &api_wallet.address, ApiWalletType::SubAccount).await?;

        Ok(())
    }

    pub(crate) async fn expand_address(
        address_allock_type: &AddressAllockType,
        index: Option<i32>,
        uid: &str,
        chain_code: &str,
        number: u32,
        serial_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;

        let password = ApiWalletDomain::get_passwd().await?;

        match address_allock_type {
            AddressAllockType::ChaBatch => {
                ApiAccountDomain::create_sub_account(
                    &api_wallet.address,
                    &password,
                    vec![chain_code.to_string()],
                    "name",
                    true,
                    number,
                )
                .await?; // 查询已有的账户
            }
            AddressAllockType::ChaIndex => {
                // 扩容一个链地址
                if let Some(index) = index {
                    ApiAccountDomain::create_api_account(
                        &api_wallet.address,
                        &password,
                        vec![chain_code.to_string()],
                        vec![index],
                        "name",
                        true,
                        ApiWalletType::SubAccount,
                    )
                    .await?;
                }
            }
        }

        let backend = CONTEXT.get().unwrap().get_global_backend_api();
        backend.expand_address_complete(uid, serial_no).await?;
        Ok(())
    }

    pub(crate) async fn get_passwd() -> Result<String, ServiceError> {
        let password = crate::infrastructure::GLOBAL_CACHE
            .get::<String>(crate::infrastructure::WALLET_PASSWORD)
            .await
            .ok_or(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::PasswordNotCached,
            ))?;
        Ok(password)
    }

    pub(crate) async fn set_passwd(wallet_password: &str) -> Result<(), ServiceError> {
        crate::infrastructure::GLOBAL_CACHE
            .set(crate::infrastructure::WALLET_PASSWORD, wallet_password)
            .await?;
        Ok(())
    }

    /// 设置uid为api钱包
    pub(crate) async fn set_api_wallet(
        sn: &str,
        recharge_uid: Option<&str>,
        withdrawal_uid: Option<&str>,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut req = AppIdImportReq::new(sn);
        if let Some(recharge_uid) = recharge_uid {
            req.set_recharge_uid(recharge_uid);
        }
        if let Some(withdrawal_uid) = withdrawal_uid {
            req.set_withdrawal_uid(withdrawal_uid);
        }
        backend.init_api_wallet(req).await?;
        Ok(())
    }

    pub(crate) async fn keys_init(
        uid: &str,
        device: &DeviceEntity,
        wallet_name: &str,
        invite_code: Option<String>,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let status = ConfigDomain::get_keys_reset_status().await?;
        if let Some(status) = status
            && let Some(false) = status.status
        {
            return Err(crate::error::business::BusinessError::Config(
                crate::error::business::config::ConfigError::KeysNotReset,
            )
            .into());
        }

        let client_id = DeviceDomain::client_id_by_device(&device)?;
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
            &uid,
            &device.sn,
            Some(client_id),
            Some(device.device_type.clone()),
            wallet_name,
            invite_code,
        );

        backend.old_keys_init(&keys_init_req).await?;
        ApiWalletRepo::mark_init(&pool, uid).await?;
        Ok(())
    }

    pub(crate) async fn check_keys_uid(uid: &str) -> Result<KeysUidCheckRes, ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let uid_check = backend.keys_uid_check(&uid).await?;

        Ok(uid_check)
    }

    /// 查询绑定信息
    pub(crate) async fn query_uid_bind_info(
        uid: &str,
    ) -> Result<QueryUidBindInfoRes, ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend.query_uid_bind_info(uid).await?)
    }

    /// 扫码绑定
    pub(crate) async fn scan_bind(
        recharge_uid: &str,
        withdrawal_uid: &str,
        org_app_id: &str,
        sn: &str,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend
            .wallet_bind_appid(&BindAppIdReq::new(recharge_uid, withdrawal_uid, org_app_id, sn))
            .await?)
    }

    /// 导入钱包
    pub(crate) async fn appid_import(
        sn: &str,
        recharge_uid: Option<&str>,
        withdrawal_uid: Option<&str>,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut req = AppIdImportReq::new(sn);
        if let Some(recharge_uid) = recharge_uid {
            req.set_recharge_uid(recharge_uid);
        }
        if let Some(withdrawal_uid) = withdrawal_uid {
            req.set_withdrawal_uid(withdrawal_uid);
        }

        Ok(backend.appid_import(req).await?)
    }

    // /// 导入子账户钱包
    // pub(crate) async fn import_sub_account_wallet(
    //     sn: &str,
    //     recharge_uid: &str,
    // ) -> Result<(), ServiceError> {
    //     let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
    //     Ok(backend.appid_sub_account_import(sn, recharge_uid).await?)
    // }

    /// 查询激活信息
    pub async fn query_wallet_activation_info(
        wallet_address: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        Ok(backend_api.query_wallet_activation_info(&api_wallet.uid).await?)
    }

    pub async fn get_api_wallet_list() -> Result<ApiWalletList, crate::error::service::ServiceError>
    {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let li = ApiWalletRepo::list(&pool, None).await?;
        let mut list = ApiWalletList::new();
        let balance_list = crate::infrastructure::asset_calc::get_wallet_balance_list().await?;
        tracing::info!("get_api_wallet_list balance_list: {balance_list:#?}");
        for e in &li {
            let mut wallet: crate::response_vo::api_wallet::wallet::WalletInfo = e.into();
            if let Some(balance) = balance_list.get(&e.address) {
                wallet = wallet.with_balance(balance.clone());
            };
            match e.api_wallet_type {
                ApiWalletType::InvalidValue => todo!(),
                ApiWalletType::SubAccount => {
                    // 如果是收款钱包，看list有没有绑定地址，有就修改，没有就不管
                    if let Some(binding_address) = &e.binding_address
                        && let Some(item) = list.iter_mut().find(|item| {
                            item.withdraw_wallet
                                .as_ref()
                                .map(|w| &w.address == binding_address)
                                .unwrap_or(false)
                        })
                    {
                        item.recharge_wallet = Some(wallet);
                    } else {
                        list.push(ApiWalletItem {
                            recharge_wallet: Some(wallet),
                            withdraw_wallet: None,
                        });
                    }
                }
                ApiWalletType::Withdrawal => {
                    if let Some(binding_address) = &e.binding_address
                        && let Some(item) = list.iter_mut().find(|item| {
                            item.recharge_wallet
                                .as_ref()
                                .map(|r| &r.address == binding_address)
                                .unwrap_or(false)
                        })
                    {
                        item.withdraw_wallet = Some(wallet);
                    } else {
                        list.push(ApiWalletItem {
                            recharge_wallet: None,
                            withdraw_wallet: Some(wallet),
                        });
                    }
                }
            }
        }
        Ok(list)
    }
}
