use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, address_query_state::AddressQueryStateRepo,
            chain::ApiChainRepo, wallet::ApiWalletRepo,
        },
        device::DeviceRepo,
        wallet::WalletRepo,
    },
};
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::{
    request::{
        DeviceDeleteReq, LanguageInitReq,
        api_wallet::{address::AddressListReq, swap::ApiInitSwapReq},
    },
    response_vo::api_wallet::wallet::{
        QueryUidBindInfoRes, QueryWalletActivationInfoResp, UidStatus,
    },
};
use wallet_tree::api::KeystoreApi;

use crate::{
    api::ReturnType,
    context::Context,
    domain::{
        api_wallet::{
            account::ApiAccountDomain, adapter_factory::ApiChainAdapterFactory,
            chain::ApiChainDomain, wallet::ApiWalletDomain,
        },
        app::{DeviceDomain, mqtt::MqttDomain},
        wallet::WalletDomain,
    },
    error::{
        business::{BusinessError, api_wallet::ApiWalletError},
        service::ServiceError,
    },
    infrastructure::task_queue::{
        CommonTask, RecoverDataBody,
        backend::{BackendApiTask, BackendApiTaskData},
        initialization::InitializationTask,
        task::Tasks,
    },
    response_vo::api_wallet::wallet::ApiWalletList,
};

pub struct ApiWalletService {
    ctx: &'static Context,
}

impl ApiWalletService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn init_api_swap(&self) -> ReturnType<()> {
        if self.ctx.is_init_api_swap().await {
            tracing::warn!("init_api_swap already initialized, skip re-init");
            return Ok(());
        }

        let backend = self.ctx.get_global_backend_api();
        let req = ApiInitSwapReq {
            sn: self.ctx.get_sn().to_string(),
            client_pub_key: GLOBAL_KEY.secret_pub_key(),
        };
        let res = backend.init_swap(&req).await?;
        if let Some(data) = res.data {
            GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
        }

        // 初始化API钱包MQTT
        #[cfg(feature = "api-mqtt")]
        MqttDomain::init_api_mqtt().await?;
        self.ctx.set_init_api_swap(true).await;

        tracing::info!(
            "init api swap successful=================================================="
        );

        tokio::spawn(async move {
            if let Err(e) = Self::init_data().await {
                tracing::error!("初始化数据失败: {:?}", e);
            }
        });

        Ok(())
    }

    async fn init_data() -> ReturnType<()> {
        // 初始化API_CHAIN_ADAPTER_FACTORY全局单例
        tracing::info!("初始化API_CHAIN_ADAPTER_FACTORY全局单例");
        let factory = ApiChainAdapterFactory::get_instance();
        // 预初始化所有链和节点的适配器
        tracing::info!("预初始化所有链和节点的适配器");
        factory.pre_init_all_adapters().await?;

        ApiChainDomain::init_api_chain_info().await?;
        Tasks::new().push(InitializationTask::PullApiWalletCoins).send().await?;
        Ok(())
    }

    pub async fn get_api_wallet_list(&self) -> ReturnType<ApiWalletList> {
        ApiWalletDomain::get_api_wallet_list_v2().await
    }

    pub async fn create_wallet(
        self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        // account_name: &str,
        // is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
        api_wallet_type: ApiWalletType,
        binding_address: Option<&str>,
    ) -> Result<String, ServiceError> {
        let start = std::time::Instant::now();

        let password_validation_start = std::time::Instant::now();
        // WalletDomain::validate_password(wallet_password).await?;
        ApiWalletDomain::cache_passwd(wallet_password).await?;

        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());
        let pool = self.ctx.api_wallet_pool()?;
        let core_pool = self.ctx.core_pool()?;

        let sn = self.ctx.get_sn();
        let password_proof = WalletDomain::generate_password_proof(wallet_password).await?;
        DeviceRepo::update_password_proof(core_pool.clone(), sn, Some(&password_proof)).await?;
        let Some(device) = DeviceRepo::get_device_info(core_pool.clone(), sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo { private_key: _, seed, address, phrase } =
            wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        let address = &address.to_string();

        if ApiWalletDomain::check_normal_wallet_exist(address).await? {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::MnemonicAlreadyImportedIntoNormalWalletSystem.into(),
            )
            .into());
        }

        tracing::debug!("Master key generation took: {:?}", master_key_start.elapsed());

        // let uid = wallet_utils::md5(&format!("{phrase}{salt}"));
        let pbkdf2_string_start = std::time::Instant::now();
        let uid = wallet_utils::pbkdf2_string(&format!("{phrase}{salt}"), salt, 100000, 32)?;

        // 检查是否是普通钱包
        let status = ApiWalletDomain::check_keys_uid(&uid).await?;
        if status.is_normal_wallet() {
            return Err(ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::MnemonicAlreadyImportedIntoNormalWalletSystem.into(),
            )));
        }

        tracing::debug!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());

        tracing::info!(
            "[import_wallet] Wallet type check completed, wallet type: {:?}, uid: {}",
            api_wallet_type,
            uid
        );

        let seed = seed.clone();

        let initialize_root_keystore_start = std::time::Instant::now();

        let (recharge_uid, withdrawal_uid) = match api_wallet_type {
            ApiWalletType::SubAccount => (Some(uid.as_str()), None),
            ApiWalletType::Withdrawal => (None, Some(uid.as_str())),
        };
        ApiWalletDomain::set_api_wallet(&device.sn, recharge_uid, withdrawal_uid).await?;

        let old = match api_wallet_type {
            ApiWalletType::SubAccount => None,
            ApiWalletType::Withdrawal => {
                if let Some(binding_address) = binding_address {
                    let recharge_wallet =
                        ApiWalletRepo::find_by_address(&pool, binding_address).await?;
                    if let Some(recharge_wallet) = recharge_wallet {
                        if let Some(binding_address) = recharge_wallet.binding_address {
                            Some(binding_address)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };

        ApiWalletDomain::upsert_api_wallet(
            &uid,
            wallet_name,
            address,
            wallet_password,
            &phrase,
            &seed,
            api_wallet_type,
            binding_address,
        )
        .await?;

        tracing::info!(
            "[import_wallet] API wallet inserted/updated successfully, uid: {}, wallet_name: {}",
            uid,
            wallet_name
        );

        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );

        match api_wallet_type {
            ApiWalletType::SubAccount => {
                // let info = ApiWalletDomain::query_uid_bind_info(&uid).await?;
                // ApiWalletDomain::bind_uid_with_app_id(
                //     address,
                //     &info.org_id,
                //     Some(info.app_id.as_str()),
                // )
                // .await?;
            }
            ApiWalletType::Withdrawal => {
                if let Some(old) = old {
                    ApiWalletRepo::physical_delete(&pool, &[&old]).await?;
                }
            }
        }

        let client_id = DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;
        ApiWalletDomain::keys_init(&uid, &device, wallet_name, invite_code).await?;

        match api_wallet_type {
            ApiWalletType::SubAccount => {
                let info = ApiWalletDomain::query_uid_bind_info(&uid).await?;
                if info.bind_status {
                    ApiWalletDomain::bind_uid_with_app_id(
                        address,
                        &info.org_id,
                        Some(info.app_id.as_str()),
                    )
                    .await?;
                }
            }
            ApiWalletType::Withdrawal => {
                if let Some(binding_address) = binding_address {
                    let recharge_wallet =
                        ApiWalletRepo::find_by_address(&pool, binding_address).await?;

                    if let Some(recharge_wallet) = recharge_wallet {
                        let info =
                            ApiWalletDomain::query_uid_bind_info(&recharge_wallet.uid).await?;

                        if info.bind_status {
                            ApiWalletDomain::appid_import(
                                sn,
                                Some(&recharge_wallet.uid),
                                Some(&uid),
                            )
                            .await?;
                            ApiWalletDomain::bind_uid_with_app_id(
                                address,
                                &info.org_id,
                                Some(info.app_id.as_str()),
                            )
                            .await?;
                        }

                        ApiWalletDomain::db_save_bind_data(
                            &recharge_wallet.address,
                            &address,
                            &info.org_id,
                            &info.app_id,
                        )
                        .await?;
                        ApiWalletDomain::db_save_sn_data(
                            &recharge_wallet.address,
                            Some(address),
                            sn,
                        )
                        .await?;

                        if info.bind_status {
                            let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
                            let chains: Vec<String> = default_chain_list
                                .iter()
                                .map(|chain| chain.chain_code.clone())
                                .collect();
                            ApiAccountDomain::create_withdrawal_account(
                                address, chains, "账户", true, false,
                            )
                            .await?;
                        }
                    }
                }
            }
        }

        ApiWalletRepo::update_sn(&pool, &address, sn).await?;

        Tasks::new()
            // .push(BackendApiTask::BackendApi(keys_init_task_data))
            .push(BackendApiTask::BackendApi(language_init_task_data))
            .send()
            .await?;

        tracing::debug!("cose time: {}", start.elapsed().as_millis());
        Ok(uid)
    }

    pub async fn import_wallet(
        self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        // account_name: &str,
        // is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
        api_wallet_type: ApiWalletType,
        binding_address: Option<&str>,
    ) -> Result<String, crate::error::service::ServiceError> {
        tracing::info!(
            "[import_wallet] Start importing API wallet, type: {:?}, name: {}",
            api_wallet_type,
            wallet_name
        );

        let password_validation_start = std::time::Instant::now();
        // WalletDomain::validate_password(wallet_password).await?;
        ApiWalletDomain::cache_passwd(wallet_password).await?;

        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());

        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let core_pool = self.ctx.core_pool()?;
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let Some(device) = DeviceRepo::get_device_info(core_pool.clone(), sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        tracing::info!("[import_wallet] Device info retrieved successfully, sn: {}", device.sn);

        // 检查是否是api钱包，是就恢复，不是就报错
        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo { private_key: _, seed, address, phrase } =
            wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        let address = &address.to_string();

        tracing::info!(
            "[import_wallet] Master key information generated successfully, wallet address: {}",
            address
        );

        // 1.校验uid，是否本地已有普通钱包
        if ApiWalletDomain::check_normal_wallet_exist(address).await? {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::AlreadyImported.into(),
            )
            .into());
        }

        tracing::debug!("Master key generation took: {:?}", master_key_start.elapsed());

        // let uid = wallet_utils::md5(&format!("{phrase}{salt}"));
        let pbkdf2_string_start = std::time::Instant::now();
        let uid = wallet_utils::pbkdf2_string(&format!("{phrase}{salt}"), salt, 100000, 32)?;
        tracing::debug!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());

        // 检查钱包类型和后端是否一致，不一致就报错
        let status = ApiWalletDomain::check_keys_uid(&uid).await?;

        if status.is_not_found() {
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            ));
        }
        match api_wallet_type {
            ApiWalletType::SubAccount => {
                if !status.is_sub_account_wallet() {
                    return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::wallet::WalletError::ImportNotSupportedForThisWalletType.into(),
                            )));
                }
            }
            ApiWalletType::Withdrawal => {
                if !status.is_withdrawal_wallet() {
                    return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::wallet::WalletError::ImportNotSupportedForThisWalletType.into(),
                            )));
                }

                if let Some(binding_address) = binding_address {
                    let recharge_wallet =
                        ApiWalletRepo::find_by_address(&pool, binding_address).await?;

                    if let Some(recharge_wallet) = recharge_wallet {
                        let info =
                            ApiWalletDomain::query_uid_bind_info(&recharge_wallet.uid).await?;
                        if !info.app_id.is_empty()
                            && !ApiWalletDomain::appid_uid_usage(
                                &info.app_id,
                                &uid,
                                UidStatus::ApiWaw,
                            )
                            .await?
                        {
                            // 该出款钱包未在该appId下使用过
                            return Err(
                                BusinessError::ApiWallet(
                                    ApiWalletError::Wallet(
                                        crate::error::business::api_wallet::wallet::WalletError
                                        ::WithdrawalWalletNotUsed
                                    )
                                ).into()
                            );
                        }
                    }
                }
            }
        }
        let seed = seed.clone();
        let initialize_root_keystore_start = std::time::Instant::now();

        let (recharge_uid, withdrawal_uid) = match api_wallet_type {
            ApiWalletType::SubAccount => (Some(uid.as_str()), None),
            ApiWalletType::Withdrawal => (None, Some(uid.as_str())),
        };

        ApiWalletDomain::set_api_wallet(&device.sn, recharge_uid, withdrawal_uid).await?;

        tracing::info!(
            "[import_wallet] API wallet settings completed, wallet type: {:?}, recharge_uid: {:?}, withdrawal_uid: {:?}",
            api_wallet_type,
            recharge_uid,
            withdrawal_uid
        );

        ApiWalletDomain::upsert_api_wallet(
            &uid,
            wallet_name,
            address,
            wallet_password,
            &phrase,
            &seed,
            api_wallet_type,
            binding_address,
        )
        .await?;

        tracing::info!(
            "[import_wallet] API wallet inserted/updated successfully, uid: {}, wallet_name: {}",
            uid,
            wallet_name
        );

        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );

        let client_id = DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;

        ApiWalletDomain::keys_init(&uid, &device, wallet_name, invite_code).await?;

        match api_wallet_type {
            ApiWalletType::SubAccount => {
                let info = ApiWalletDomain::query_uid_bind_info(&uid).await?;

                ApiWalletDomain::bind_uid_with_app_id(
                    address,
                    &info.org_id,
                    Some(info.app_id.as_str()),
                )
                .await?;

                if info.bind_status {
                    ApiWalletDomain::appid_import_recharge_wallet(sn, &uid).await?;
                }
            }
            ApiWalletType::Withdrawal => {
                if let Some(binding_address) = binding_address {
                    let recharge_wallet =
                        ApiWalletRepo::find_by_address(&pool, binding_address).await?;

                    if let Some(recharge_wallet) = recharge_wallet {
                        let info =
                            ApiWalletDomain::query_uid_bind_info(&recharge_wallet.uid).await?;

                        if info.bind_status {
                            ApiWalletDomain::appid_import(
                                sn,
                                Some(&recharge_wallet.uid),
                                Some(&uid),
                            )
                            .await?;
                            ApiWalletDomain::bind_uid_with_app_id(
                                address,
                                &info.org_id,
                                Some(info.app_id.as_str()),
                            )
                            .await?;
                        }

                        ApiWalletDomain::db_save_bind_data(
                            &recharge_wallet.address,
                            &address,
                            &info.org_id,
                            &info.app_id,
                        )
                        .await?;
                        ApiWalletDomain::db_save_sn_data(
                            &recharge_wallet.address,
                            Some(address),
                            sn,
                        )
                        .await?;
                    }
                }

                let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
                let chains: Vec<String> =
                    default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
                ApiAccountDomain::create_withdrawal_account(address, chains, "账户", true, false)
                    .await?;
            }
        }

        ApiWalletRepo::update_sn(&pool, &address, sn).await?;

        let mut tasks = Tasks::new();
        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();

        let wallet = ApiWalletRepo::find_by_uid(&pool, &uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        if wallet.app_id.is_some() {
            for chain_code in chains {
                let query_address_list_req = AddressListReq::new(&uid, &chain_code, 0, 1000);

                let query_address_list_task_data = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
                    &query_address_list_req,
                )?;
                tasks = tasks.push(BackendApiTask::BackendApi(query_address_list_task_data));
            }
        }

        tracing::info!("[import_wallet] Sending backend tasks");

        tasks
            // .push(BackendApiTask::BackendApi(keys_init_task_data))
            .push(BackendApiTask::BackendApi(language_init_task_data))
            .send()
            .await?;

        tracing::info!("[import_wallet] Backend tasks sent successfully");
        tracing::info!("[import_wallet] Wallet import completed successfully, uid: {}", uid);

        Ok(uid)
    }

    pub async fn scan_bind(
        self,
        app_id: &str,
        org_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::get_context()?.api_wallet_pool()?;
        let sn = self.ctx.get_sn();

        let recharge_wallet = ApiWalletRepo::find_by_uid(&pool, recharge_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        let withdrawal_wallet = ApiWalletRepo::find_by_uid(&pool, withdrawal_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        ApiWalletDomain::scan_bind(recharge_uid, withdrawal_uid, app_id, sn).await?;
        ApiWalletDomain::db_save_bind_data(
            &recharge_wallet.address,
            &withdrawal_wallet.address,
            org_id,
            app_id,
        )
        .await?;
        ApiWalletDomain::db_save_sn_data(
            &recharge_wallet.address,
            Some(&withdrawal_wallet.address),
            &sn,
        )
        .await?;

        tracing::info!(sn=%sn, "sn ------------ ==============================");
        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        if chains.is_empty() {
            tracing::warn!("scan_bind is empty");
        }
        ApiAccountDomain::create_withdrawal_account(
            &withdrawal_wallet.address,
            chains,
            "账户",
            true,
            false,
        )
        .await?;

        tracing::info!("bind merchant success");
        Ok(())
    }

    pub async fn import_bind(
        self,
        sn: &str,
        org_id: &str,
        app_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::get_context()?.api_wallet_pool()?;

        let recharge_wallet = ApiWalletRepo::find_by_uid(&pool, recharge_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        let withdrawal_wallet = ApiWalletRepo::find_by_uid(&pool, withdrawal_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        ApiWalletDomain::appid_import(sn, Some(recharge_uid), Some(withdrawal_uid)).await?;

        ApiWalletDomain::db_save_bind_data(
            &recharge_wallet.address,
            &withdrawal_wallet.address,
            org_id,
            app_id,
        )
        .await?;

        ApiWalletDomain::db_save_sn_data(
            &recharge_wallet.address,
            Some(&withdrawal_wallet.address),
            sn,
        )
        .await?;

        // let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        // let chains: Vec<String> =
        //     default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        // ApiAccountDomain::create_withdrawal_account(
        //     &withdrawal_wallet.address,
        //     chains,
        //     "账户",
        //     true,
        //     false,
        // )
        // .await?;
        Ok(())
    }

    pub async fn unbind_merchant(
        self,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiWalletDomain::unbind_uid(recharge_uid).await?;
        ApiWalletDomain::unbind_uid(withdrawal_uid).await?;

        todo!();
        // let backend = crate::Context::get_global_backend_api()?;
        // backend
        //     .wallet_bind_appid(&BindAppIdReq::new(recharge_uid, withdrawal_uid, org_app_id))
        //     .await?;
        Ok(())
    }

    pub async fn edit_wallet_name(
        self,
        address: &str,
        name: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        ApiWalletRepo::edit_name(&pool, address, name).await?;
        Ok(())
    }

    pub async fn set_passwd_cache(
        self,
        wallet_password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        WalletDomain::validate_password(wallet_password).await?;
        ApiWalletDomain::cache_passwd(wallet_password).await?;
        crate::infrastructure::system_ready::mark_system_ready();

        Tasks::new().push(InitializationTask::CacheSeed).send().await?;

        Ok(())
    }

    pub async fn query_wallet_activation_info(
        self,
        wallet_address: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
        ApiWalletDomain::query_wallet_activation_info(wallet_address).await
    }

    pub async fn get_phrase(
        &mut self,
        wallet_address: &str,
        password: &str,
    ) -> Result<
        crate::response_vo::standard_wallet::wallet::GetPhraseRes,
        crate::error::service::ServiceError,
    > {
        let pool = crate::get_context()?.api_wallet_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        let phrase = ApiWalletDomain::decrypt_phrase(password, &api_wallet.phrase).await?;

        Ok(crate::response_vo::standard_wallet::wallet::GetPhraseRes { phrase })
    }

    pub async fn physical_delete(
        self,
        address: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::get_context()?.api_wallet_pool()?;
        let core_pool = self.ctx.core_pool()?;
        let wallet = ApiWalletRepo::find_by_address(&pool, address).await?;

        ApiWalletRepo::physical_delete(&pool, &[address]).await?;
        if let Some(wallet) = &wallet {
            AddressQueryStateRepo::delete_by_uid(&pool, &wallet.uid).await?;
        }

        let mut accounts = ApiAccountRepo::physical_delete_all(&pool, &[address]).await?;

        if let Some(wallet) = &wallet
            && wallet.api_wallet_type == ApiWalletType::SubAccount
            && let Some(binding_address) = &wallet.binding_address
        {
            let withdraw_wallet = ApiWalletRepo::find_by_address(&pool, binding_address).await?;
            if let Some(withdraw_wallet) = withdraw_wallet {
                AddressQueryStateRepo::delete_by_uid(&pool, &withdraw_wallet.uid).await?;
            }

            let withdraw_wallet = ApiWalletRepo::physical_delete(&pool, &[binding_address]).await?;
            let mut uids: Vec<String> =
                withdraw_wallet.into_iter().map(|withdraw| withdraw.uid).collect();
            uids.push(wallet.uid.to_string());
            let withdraw_accounts =
                ApiAccountRepo::physical_delete_all(&pool, &[binding_address]).await?;
            accounts.extend(withdraw_accounts);
        }

        let sn = crate::get_context()?.get_sn();

        let dirs = crate::get_context()?.get_global_dirs();

        let latest_wallet = ApiWalletRepo::wallet_latest(&pool).await?;

        let rest_api_uids = ApiWalletRepo::uid_list(&pool)
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();

        let rest_standard_uids = WalletRepo::uid_list(core_pool.clone())
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();

        // Check if both standard wallets and API wallets are empty before consuming the vectors
        let has_standard_wallets = !rest_standard_uids.is_empty();
        let has_api_wallets = !rest_api_uids.is_empty();

        let rest_uids =
            rest_standard_uids.into_iter().chain(rest_api_uids).collect::<Vec<String>>();

        let uid = if let Some(latest_wallet) = latest_wallet {
            Some(latest_wallet.uid)
        } else {
            // Only remove verify file if both standard wallets and API wallets are deleted
            if !has_standard_wallets && !has_api_wallets {
                KeystoreApi::remove_verify_file(&dirs.root_dir)?;
                DeviceRepo::update_password_proof(core_pool.clone(), sn, None).await?;
                ApiWalletDomain::clear_passwd().await?;
            }

            // tx.update_password(None).await?;
            None
        };

        DeviceRepo::update_uid(core_pool, sn, uid.as_deref()).await?;

        if let Some(wallet) = wallet {
            let req = DeviceDeleteReq::new(sn, &rest_uids);

            let device_unbind_address_task =
                DeviceDomain::gen_device_unbind_all_api_address_task_data(accounts.as_slice(), sn)
                    .await?;
            // FIXME: 这里的任务执行时间不能保证，比后续的设备初始化等接口快执行，所以暂时先用同步处理
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            backend.device_delete(&req).await?;

            Tasks::new()
                // .push(BackendApiTask::BackendApi(BackendApiTaskData::new(
                //     endpoint::DEVICE_DELETE,
                //     &req,
                // )?))
                .push(BackendApiTask::BackendApi(device_unbind_address_task))
                .send()
                .await?;
        };

        // find tron address and del permission

        for uid in rest_uids {
            let body = RecoverDataBody::new(&uid);

            Tasks::new().push(CommonTask::RecoverMultisigAccountData(body)).send().await?;
        }
        Ok(())
    }

    // pub async fn appid_withdrawal_wallet_change(
    //     &self,
    //     withdrawal_uid: &str,
    //     org_app_id: &str,
    // ) -> Result<(), crate::error::service::ServiceError> {
    //     let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
    //     backend.appid_withdrawal_wallet_change(withdrawal_uid, org_app_id).await?;
    //     Ok(())
    // }

    pub async fn change_withdrawal_wallet(
        &self,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiWalletDomain::change_withdrawal_wallet(recharge_uid, withdrawal_uid).await
    }

    pub async fn query_uid_bind_info(
        &self,
        uid: &str,
    ) -> Result<QueryUidBindInfoRes, crate::error::service::ServiceError> {
        ApiWalletDomain::query_uid_bind_info(uid).await
    }

    pub async fn is_wallet_authorized_on_device(
        &self,
        wallet_address: &str,
    ) -> Result<bool, crate::error::service::ServiceError> {
        let sn = self.ctx.get_sn();
        ApiWalletDomain::is_wallet_authorized_on_device(wallet_address, sn).await
    }

    //     pub async fn physical_delete(self, address: &str) -> Result<(), crate::ServiceError> {
    //         let mut tx = self.repo;

    //         tx.begin_transaction().await?;
    //         let wallet = tx.wallet_detail_by_address(address).await?;
    //         ApiWalletRepo::delete(&mut tx, &[address]).await?;
    //         let accounts = AccountRepoTrait::physical_delete_all(&mut tx, &[address]).await?;
    //         let Some(device) = tx.get_device_info().await? else {
    //             return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
    //         };
    //         let dirs = crate::manager::Context::get_global_dirs()?;
    //         let wallet_dir = dirs.get_wallet_dir(Some(address));
    //         wallet_utils::file_func::remove_dir_all(wallet_dir)?;

    //         let latest_wallet = tx.wallet_latest().await?;

    //         let rest_uids = tx
    //             .uid_list()
    //             .await?
    //             .into_iter()
    //             .map(|uid| uid.0)
    //             .collect::<Vec<String>>();

    //         let uid = if let Some(latest_wallet) = latest_wallet {
    //             Some(latest_wallet.uid)
    //         } else {
    //             KeystoreApi::remove_verify_file(&dirs.root_dir)?;
    //             tx.update_password(None).await?;
    //             None
    //         };
    //         tx.update_uid(uid.as_deref()).await?;
    //         tx.commit_transaction().await?;
    //         let pool = crate::Context::api_wallet_pool()?;

    //         if let Some(wallet) = wallet {
    //             let req = DeviceDeleteReq::new(&device.sn, &rest_uids);
    //             let device_delete_task = Task::BackendApi(BackendApiTask::BackendApi(
    //                 BackendApiTaskData::new(endpoint::DEVICE_DELETE, &req)?,
    //             ));

    //             let members = MultisigMemberDaoV1::list_by_uid(&wallet.uid, &*pool)
    //                 .await
    //                 .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?;

    //             let multisig_accounts =
    //                 MultisigDomain::physical_delete_wallet_account(members, &wallet.uid, pool.clone())
    //                     .await?;

    //             let device_unbind_address_task = DeviceDomain::gen_device_unbind_all_address_task_data(
    //                 &accounts,
    //                 multisig_accounts,
    //                 &device.sn,
    //             )
    //             .await?;

    //             let device_unbind_address_task =
    //                 Task::BackendApi(BackendApiTask::BackendApi(device_unbind_address_task));
    //             Tasks::new()
    //                 .push(device_delete_task)
    //                 .push(device_unbind_address_task)
    //                 .send()
    //                 .await?;
    //         };

    //         // find tron address and del permission
    //         let tron_address = accounts.iter().find(|a| a.chain_code == chain_code::TRON);
    //         tracing::warn!("tron address = {:?}", tron_address);
    //         if let Some(address) = tron_address {
    //             PermissionDomain::delete_by_address(&pool, &address.address).await?;
    //         }

    //         for uid in rest_uids {
    //             let body = RecoverDataBody::new(&uid);

    //             Tasks::new()
    //                 .push(Task::Common(CommonTask::RecoverMultisigAccountData(body)))
    //                 .send()
    //                 .await?;
    //         }
    //         Ok(())
    //     }
}
