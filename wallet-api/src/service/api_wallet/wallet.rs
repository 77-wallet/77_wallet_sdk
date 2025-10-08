use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        api_wallet::{chain::ApiChainRepo, wallet::ApiWalletRepo},
        device::DeviceRepo,
    },
};
use wallet_transport_backend::{
    request::{LanguageInitReq, api_wallet::address::AddressListReq},
    response_vo::api_wallet::wallet::{QueryUidBindInfoRes, QueryWalletActivationInfoResp},
};

use crate::{
    api::ReturnType,
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        app::DeviceDomain,
        wallet::WalletDomain,
    },
    error::service::ServiceError,
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    response_vo::api_wallet::wallet::{ApiWalletInfo, ApiWalletItem, ApiWalletList},
};

pub struct ApiWalletService {
    ctx: &'static Context,
}

impl ApiWalletService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn get_api_wallet_list(&self) -> ReturnType<ApiWalletList> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        let li = ApiWalletRepo::list(&pool, None).await?;
        let mut list = ApiWalletList::new();
        for e in &li {
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
                        item.recharge_wallet = Some(e.into());
                    } else {
                        list.push(ApiWalletItem {
                            recharge_wallet: Some(e.into()),
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
                        item.withdraw_wallet = Some(e.into());
                    } else {
                        list.push(ApiWalletItem {
                            recharge_wallet: None,
                            withdraw_wallet: Some(e.into()),
                        });
                    }
                }
            }
        }
        Ok(list)
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
        if api_wallet_type == ApiWalletType::InvalidValue {
            return Err(ServiceError::Database(wallet_database::Error::InvalidValue(
                api_wallet_type as u8,
            )));
        }
        let start = std::time::Instant::now();

        let password_validation_start = std::time::Instant::now();
        WalletDomain::validate_password(wallet_password).await?;
        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());

        let pool = self.ctx.get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool.clone()).await? else {
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
                crate::error::business::api_wallet::ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem,
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
            return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Wallet(
                crate::error::business::wallet::WalletError::MnemonicAlreadyImportedIntoNormalWalletSystem,
            )));
        }
        tracing::info!("status: {:?}", status);
        tracing::debug!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());
        let seed = seed.clone();

        let initialize_root_keystore_start = std::time::Instant::now();

        let (recharge_uid, withdrawal_uid) = match api_wallet_type {
            ApiWalletType::SubAccount => (Some(uid.as_str()), None),
            ApiWalletType::Withdrawal => (None, Some(uid.as_str())),
            _ => (None, None),
        };
        ApiWalletDomain::set_api_wallet(&device.sn, recharge_uid, withdrawal_uid).await?;
        tracing::info!("init api wallet success");

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
        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );
        // let default_chain_list = ChainRepo::get_chain_list(&pool).await?;

        // let chains: Vec<String> =
        //     default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        // match api_wallet_type {
        //     ApiWalletType::SubAccount => {
        //         ApiWalletDomain::create_sub_account(
        //             address,
        //             wallet_password,
        //             chains,
        //             account_name,
        //             is_default_name,
        //         )
        //         .await?
        //     }
        //     ApiWalletType::Withdrawal => {
        //         ApiWalletDomain::create_withdrawal_account(
        //             address,
        //             wallet_password,
        //             chains,
        //             account_name,
        //             is_default_name,
        //         )
        //         .await?
        //     }
        //     _ => {}
        // }

        let client_id = DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        // let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
        //     &uid,
        //     &device.sn,
        //     Some(client_id),
        //     Some(device.device_type),
        //     wallet_name,
        //     invite_code,
        // );
        // let keys_init_task_data = BackendApiTaskData::new(
        //     wallet_transport_backend::consts::endpoint::old_wallet::OLD_KEYS_V2_INIT,
        //     &keys_init_req,
        // )?;

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;

        ApiWalletDomain::keys_init(&uid, &device, wallet_name, invite_code).await?;
        tracing::info!("[create wallet] keys init");
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
        if api_wallet_type == ApiWalletType::InvalidValue {
            return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::ImportNotSupportedForThisAccountType,
                    )));
        }
        let password_validation_start = std::time::Instant::now();
        WalletDomain::validate_password(wallet_password).await?;
        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool.clone()).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        // 检查是否是api钱包，是就恢复，不是就报错
        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo { private_key: _, seed, address, phrase } =
            wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        let address = &address.to_string();

        // 1.校验uid，是否本地已有普通钱包
        if ApiWalletDomain::check_normal_wallet_exist(address).await? {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::ImportNotSupportedForThisAccountType,
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
                    crate::error::business::api_wallet::ApiWalletError::WalletDoesNotExist,
                ),
            ));
        }

        tracing::info!("status: {status:?}");
        match api_wallet_type {
            ApiWalletType::InvalidValue => todo!(),
            ApiWalletType::SubAccount => {
                if !status.is_sub_account_wallet() {
                    return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ImportNotSupportedForThisAccountType,
                            )));
                }
            }
            ApiWalletType::Withdrawal => {
                if !status.is_withdrawal_wallet() {
                    return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::ApiWalletError::ImportNotSupportedForThisAccountType,
                            )));
                }
            }
        }

        let seed = seed.clone();

        let initialize_root_keystore_start = std::time::Instant::now();

        let (recharge_uid, withdrawal_uid) = match api_wallet_type {
            ApiWalletType::SubAccount => (Some(uid.as_str()), None),
            ApiWalletType::Withdrawal => (None, Some(uid.as_str())),
            _ => (None, None),
        };
        ApiWalletDomain::set_api_wallet(&device.sn, recharge_uid, withdrawal_uid).await?;
        tracing::info!("init api wallet success");

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
        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );

        // let default_chain_list = ChainRepo::get_chain_list(&pool).await?;

        // let chains: Vec<String> =
        //     default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        // match api_wallet_type {
        //     ApiWalletType::SubAccount => {
        //         ApiWalletDomain::create_sub_account(
        //             address,
        //             wallet_password,
        //             chains,
        //             account_name,
        //             is_default_name,
        //         )
        //         .await?
        //     }
        //     ApiWalletType::Withdrawal => {
        //         ApiWalletDomain::create_withdrawal_account(
        //             address,
        //             wallet_password,
        //             chains,
        //             account_name,
        //             is_default_name,
        //         )
        //         .await?
        //     }
        //     _ => {}
        // }

        let client_id = DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        // let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
        //     &uid,
        //     &device.sn,
        //     Some(client_id),
        //     Some(device.device_type),
        //     wallet_name,
        //     invite_code,
        // );
        // let keys_init_task_data = BackendApiTaskData::new(
        //     wallet_transport_backend::consts::endpoint::old_wallet::OLD_KEYS_V2_INIT,
        //     &keys_init_req,
        // )?;

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;

        ApiWalletDomain::keys_init(&uid, &device, wallet_name, invite_code).await?;

        let mut tasks = Tasks::new();
        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();

        for chain_code in chains {
            let query_address_list_req = AddressListReq::new(&uid, &chain_code, 0, 1000);

            let query_address_list_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
                &query_address_list_req,
            )?;
            tasks = tasks.push(BackendApiTask::BackendApi(query_address_list_task_data));
        }

        tasks
            // .push(BackendApiTask::BackendApi(keys_init_task_data))
            .push(BackendApiTask::BackendApi(language_init_task_data))
            .send()
            .await?;

        Ok(uid)
    }

    pub async fn scan_bind(
        self,
        app_id: &str,
        org_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let recharge_wallet = ApiWalletRepo::find_by_uid(&pool, recharge_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        let withdrawal_wallet = ApiWalletRepo::find_by_uid(&pool, withdrawal_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        ApiWalletDomain::db_save_bind_data(
            &recharge_wallet.address,
            &withdrawal_wallet.address,
            org_id,
            app_id,
        )
        .await?;

        let Some(device) = DeviceRepo::get_device_info(pool.clone()).await? else {
            return Err(ServiceError::Business(
                crate::error::business::BusinessError::Device(
                    crate::error::business::device::DeviceError::Uninitialized,
                )
                .into(),
            ));
        };

        // ApiWalletDomain::set_api_wallet(&device.sn, Some(recharge_uid), Some(withdrawal_uid))
        //     .await?;
        // tracing::info!("init api wallet success");

        ApiWalletDomain::scan_bind(recharge_uid, withdrawal_uid, app_id, &device.sn).await?;

        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        let password = ApiWalletDomain::get_passwd().await?;
        // ApiWalletDomain::create_sub_account(
        //     &recharge_wallet.address,
        //     &password,
        //     chains.clone(),
        //     account_name,
        //     is_default_name,
        // )
        // .await?;
        ApiAccountDomain::create_withdrawal_account(
            &withdrawal_wallet.address,
            &password,
            chains,
            "账户",
            true,
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
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let recharge_wallet = ApiWalletRepo::find_by_uid(&pool, recharge_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        let withdrawal_wallet = ApiWalletRepo::find_by_uid(&pool, withdrawal_uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        ApiWalletDomain::db_save_bind_data(
            &recharge_wallet.address,
            &withdrawal_wallet.address,
            org_id,
            app_id,
        )
        .await?;

        ApiWalletDomain::appid_import(sn, Some(recharge_uid), Some(withdrawal_uid)).await?;

        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        let password = ApiWalletDomain::get_passwd().await?;
        ApiAccountDomain::create_withdrawal_account(
            &withdrawal_wallet.address,
            &password,
            chains,
            "账户",
            true,
        )
        .await?;
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
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWalletRepo::edit_name(&pool, address, name).await?;
        Ok(())
    }

    pub async fn set_passwd_cache(
        self,
        wallet_password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiWalletDomain::set_passwd(wallet_password).await?;
        Ok(())
    }

    pub async fn query_wallet_activation_info(
        self,
        wallet_address: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
        ApiWalletDomain::query_wallet_activation_info(wallet_address).await
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

    pub async fn query_uid_bind_info(
        &self,
        uid: &str,
    ) -> Result<QueryUidBindInfoRes, crate::error::service::ServiceError> {
        ApiWalletDomain::query_uid_bind_info(uid).await
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
    //         let pool = crate::Context::get_global_sqlite_pool()?;

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
