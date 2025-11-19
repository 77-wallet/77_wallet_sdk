use std::collections::HashSet;

use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::{api_wallet::ApiWalletType, device::DeviceEntity},
    repositories::{
        api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo},
        task_queue::TaskQueueRepo,
        wallet::WalletRepo,
    },
};
use wallet_transport_backend::{
    request::api_wallet::{
        address::ExpandAddressCompleteReq,
        wallet::{AppIdImportReq, AppIdUidUsageReq, BindAppIdReq},
    },
    response_vo::api_wallet::wallet::{
        KeysUidCheckRes, QueryUidBindInfoRes, QueryWalletActivationInfoResp, UidStatus,
    },
};

use crate::{
    context::CONTEXT,
    domain::{
        api_wallet::account::ApiAccountDomain,
        app::{DeviceDomain, config::ConfigDomain},
    },
    error::service::ServiceError,
    messaging::mqtt::topics::api_wallet::cmd::address_allock::{AddressAllockType, ExpandStatus},
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

        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        ApiWalletRepo::upsert(
            &pool,
            &uid,
            wallet_name,
            wallet_address,
            &phrase_enc,
            &seed_enc,
            api_wallet_type,
            binding_address,
            sn,
        )
        .await?;

        tracing::info!("upsert api wallet uid: {:?}", uid);
        if let Some(binding_address) = binding_address {
            if api_wallet_type == ApiWalletType::Withdrawal {
                let recharge_wallet =
                    ApiWalletRepo::find_by_address(&pool, binding_address).await?;

                if let Some(recharge_wallet) = recharge_wallet {
                    // let info = ApiWalletDomain::query_uid_bind_info(&recharge_wallet.uid).await?;
                    // if info.bind_status {
                    //     let backend = CONTEXT.get().unwrap().get_global_backend_api();
                    //     backend.appid_withdrawal_wallet_change(uid, &info.app_id).await?;
                    // }

                    if let Some(address) = recharge_wallet.binding_address {
                        tracing::info!("address: {address}, wallet_address: {wallet_address}");
                        if address != wallet_address {
                            ApiWalletRepo::unbind_uid(&pool, &address).await?;
                            Self::bind_uid_with_app_id(
                                &wallet_address,
                                &recharge_wallet.merchant_id,
                                recharge_wallet.app_id.as_deref(),
                                // &recharge_wallet.sn,
                            )
                            .await?;
                        }
                    }
                }
            }

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
        ApiWalletDomain::bind_uid_with_app_id(recharge_address, org_id, Some(app_id)).await?;
        ApiWalletDomain::bind_uid_with_app_id(withdrawal_address, org_id, Some(app_id)).await?;
        ApiWalletDomain::bind_withdraw_and_subaccount_relation(
            recharge_address,
            withdrawal_address,
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn bind_uid_with_app_id(
        address: &str,
        merchain_id: &str,
        org_app_id: Option<&str>,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWalletRepo::update_merchant_id(&pool, &address, merchain_id).await?;
        ApiWalletRepo::update_app_id(&pool, &address, org_app_id).await?;

        Ok(())
    }

    pub(crate) async fn db_save_sn_data(
        recharge_address: &str,
        withdrawal_address: Option<&str>,
        sn: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiWalletRepo::update_sn(&pool, &recharge_address, sn).await?;
        if let Some(withdrawal_address) = withdrawal_address {
            ApiWalletRepo::update_sn(&pool, &withdrawal_address, sn).await?;
        }
        Ok(())
    }

    pub(crate) async fn bind_withdraw_and_subaccount_relation(
        subaccount_uid: &str,
        withdraw_uid: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        ApiWalletRepo::bind_withdraw_and_subaccount_relation(
            pool.clone(),
            &subaccount_uid,
            &withdraw_uid,
        )
        .await?;

        ApiWalletRepo::bind_withdraw_and_subaccount_relation(pool, &withdraw_uid, &subaccount_uid)
            .await?;
        Ok(())
    }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        ApiWalletRepo::unbind_uid(&pool, &api_wallet.address).await?;

        Ok(())
    }

    pub(crate) async fn unbind_uid_by_address(
        address: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        ApiWalletRepo::unbind_uid(&pool, &api_wallet.address).await?;

        Ok(())
    }

    pub(crate) async fn expand_address(
        msg_id: &str,
        address_allock_type: &AddressAllockType,
        index: Option<i32>,
        uid: &str,
        chain_code: &str,
        number: u32,
        serial_no: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let backend = CONTEXT.get().unwrap().get_global_backend_api();

        let Some(api_wallet) = ApiWalletRepo::find_by_uid(&pool, &uid).await? else {
            let req =
                ExpandAddressCompleteReq::new(uid, serial_no, false, Some("api wallet not found"));
            backend.expand_address_complete(req).await?;
            // match res {
            //     Ok(_) => {
            //         TaskQueueRepo::delete_task(&pool, msg_id).await?;
            //     }
            //     Err(ref e) => match e {
            //         wallet_transport_backend::Error::ApiBackend(code, _) => {
            //             if *code == 8660002 {
            //                 TaskQueueRepo::delete_task(&pool, msg_id).await?;
            //             }
            //         }
            //         _ => res?,
            //     },
            // }
            return Ok(());
        };

        let task = TaskQueueRepo::task_detail(&pool, msg_id).await?;
        let needed_indices = if let Some(task) = task
            && let Some(reamrk) = task.remark
        {
            let mut remark = wallet_utils::serde_func::serde_from_str::<ExpandStatus>(&reamrk)?;
            let res: Vec<i32> = remark.symmetric_diff().into_iter().collect();
            let mut needed_indices = Vec::new();
            let mut changed = false;
            for input_index in res {
                let account_index_map =
                    wallet_utils::address::AccountIndexMap::from_input_index(input_index)?;

                // 跳过已存在账户
                if let Some(account) =
                    ApiAccountRepo::find_one_by_wallet_address_account_id_chain_code(
                        &pool,
                        &api_wallet.address,
                        account_index_map.account_id,
                        &chain_code,
                    )
                    .await?
                {
                    if account.is_init == 1 {
                        remark.completed_indices.insert(input_index);
                        remark.status = true;
                        changed = true;
                        continue;
                    }
                    // TODO：可以加上补发上报地址逻辑
                }

                needed_indices.push(input_index);
            }
            if changed {
                let updated_remark = wallet_utils::serde_func::serde_to_string(&remark)?;
                tracing::info!("1 expand address updated_remark: {:?}", updated_remark);
                TaskQueueRepo::update_task_remark(&pool, msg_id, &updated_remark).await?;
            }

            needed_indices
        } else {
            let needed_indices = match address_allock_type {
                AddressAllockType::ChaBatch => {
                    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                    // 查询已有的账户
                    let account_indices = ApiAccountRepo::get_all_account_indices(
                        &pool,
                        &api_wallet.address,
                        chain_code,
                    )
                    .await?;
                    let account_indices =
                        ApiAccountDomain::next_account_indices(account_indices, number);
                    let mut input_indices = Vec::with_capacity(account_indices.len());
                    for account_id in account_indices {
                        input_indices.push(
                            wallet_utils::address::AccountIndexMap::from_account_id(account_id)?
                                .input_index,
                        );
                    }
                    input_indices
                }
                AddressAllockType::ChaIndex => {
                    if let Some(index) = index {
                        vec![index]
                    } else {
                        vec![]
                    }
                }
            };
            let remark = ExpandStatus::new(
                needed_indices.iter().cloned().collect(),
                HashSet::new(),
                false,
                needed_indices.len() as u32,
            );
            let updated_remark = wallet_utils::serde_func::serde_to_string(&remark)?;
            tracing::info!("2 expand address updated_remark: {:?}", updated_remark);
            TaskQueueRepo::update_task_remark(&pool, msg_id, &updated_remark).await?;
            needed_indices
        };

        tracing::info!("expand address index: {:?}", needed_indices);
        if !needed_indices.is_empty() {
            let password = ApiWalletDomain::get_passwd().await?;
            ApiAccountDomain::create_sub_account(
                &api_wallet.address,
                uid,
                &password,
                chain_code,
                "账户",
                true,
                number,
                needed_indices,
            )
            .await?;
        }

        // match address_allock_type {
        //     AddressAllockType::ChaBatch => {
        //         ApiAccountDomain::create_sub_account(
        //             &api_wallet.address,
        //             uid,
        //             &password,
        //             chain_code,
        //             "账户",
        //             true,
        //             number,
        //             indices,
        //         )
        //         .await?;
        //     }
        //     AddressAllockType::ChaIndex => {
        //         // 扩容一个链地址
        //         if let Some(index) = index {
        //             ApiAccountDomain::create_api_account(
        //                 &api_wallet.address,
        //                 &password,
        //                 vec![chain_code.to_string()],
        //                 vec![index],
        //                 "账户",
        //                 true,
        //                 ApiWalletType::SubAccount,
        //             )
        //             .await?;
        //             let data = NotifyEvent::AwmCmdAddrExpand(AwmCmdAddrExpandMsgFront {
        //                 uid: uid.to_string(),
        //                 done_number: 1,
        //                 number,
        //             });
        //             FrontendNotifyEvent::new(data).send().await?;
        //         }
        //     }
        // }

        // let req = ExpandAddressCompleteReq::new(uid, serial_no, true, None);
        // backend.expand_address_complete(req).await?;
        Ok(())
    }

    pub(crate) async fn get_passwd() -> Result<String, ServiceError> {
        let password = crate::infrastructure::cache::GLOBAL_CACHE
            .get::<String>(crate::infrastructure::cache::WALLET_PASSWORD)
            .await
            .ok_or(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::PasswordNotCached,
            ))?;
        Ok(password)
    }

    pub(crate) async fn set_passwd(wallet_password: &str) -> Result<(), ServiceError> {
        crate::infrastructure::cache::GLOBAL_CACHE
            .set(crate::infrastructure::cache::WALLET_PASSWORD, wallet_password)
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

    pub(crate) async fn change_withdrawal_wallet(
        recharge_uid: &str,
        withdrawal_uid: &str,
        // app_id: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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
        let Some(app_id) = recharge_wallet.app_id else {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::SubAccountWalletNotBound
                    .into(),
            )
            .into());
        };
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend.appid_withdrawal_wallet_change(withdrawal_uid, &app_id).await?;
        if let Some(binding_address) = recharge_wallet.binding_address {
            ApiWalletDomain::unbind_uid_by_address(&binding_address).await?;
        }

        ApiWalletDomain::db_save_bind_data(
            &recharge_wallet.address,
            &withdrawal_wallet.address,
            &recharge_wallet.merchant_id,
            &app_id,
        )
        .await?;
        if let Some(sn) = recharge_wallet.sn {
            ApiWalletRepo::update_sn(&pool, &withdrawal_wallet.address, &sn).await?;
        }

        Ok(())
    }

    /// 查询绑定信息
    pub(crate) async fn query_uid_bind_info(
        uid: &str,
    ) -> Result<QueryUidBindInfoRes, ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend.query_uid_bind_info(uid).await?)
    }

    pub async fn is_wallet_authorized_on_device(
        wallet_address: &str,
        sn: &str,
    ) -> Result<bool, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        if let Some(_sn) = wallet.sn {
            if _sn == *sn {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 查询钱包在uid下的使用状态
    pub(crate) async fn appid_uid_usage(
        org_app_id: &str,
        uid: &str,
        wallet_type: UidStatus,
    ) -> Result<bool, ServiceError> {
        let req = AppIdUidUsageReq::new(org_app_id, uid, wallet_type);
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend.appid_uid_usage(req).await?.used)
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
        backend.appid_import(req).await?;

        Ok(())
    }

    pub(crate) async fn appid_import_recharge_wallet(
        sn: &str,
        recharge_uid: &str,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        backend.appid_import_recharge_wallet(sn, recharge_uid).await?;
        Ok(())
    }

    /// 查询激活信息
    pub async fn query_wallet_activation_info(
        wallet_address: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        Ok(backend_api.query_wallet_activation_info(&api_wallet.uid).await?)
    }

    pub async fn get_api_wallet_list() -> Result<ApiWalletList, crate::error::service::ServiceError>
    {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let li = ApiWalletRepo::list(pool.as_ref(), None).await?;
        let mut list = ApiWalletList::new();
        let balance_list = crate::infrastructure::asset_calc::get_wallet_balance_list().await?;
        // tracing::info!("get_api_wallet_list balance_list: {balance_list:#?}");
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

        // list.retain(|item| item.recharge_wallet.is_some());
        Ok(list)
    }
}
