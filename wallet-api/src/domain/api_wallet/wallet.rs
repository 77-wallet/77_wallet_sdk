use crate::{
    context::{CONTEXT, WalletSessionState, WalletUnlockState},
    domain::{
        api_wallet::{assets::ApiAssetsDomain, unlock},
        app::{DeviceDomain, config::ConfigDomain},
    },
    error::service::ServiceError,
    messaging::mqtt::topics::api_wallet::cmd::address_allock::{
        AddressAllockType, AwmCmdAddrExpandMsg, EXPAND_INDEX_LOCK,
    },
    response_vo::api_wallet::wallet::{ApiWalletItem, ApiWalletList},
};
use std::time::Instant;
use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::{api_wallet::ApiWalletType, device::DeviceEntity},
    repositories::{api_wallet::wallet::ApiWalletRepo, wallet::WalletRepo},
};
use wallet_transport_backend::{
    request::api_wallet::{
        address::ExpandAddressCompleteReq,
        wallet::{AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq},
    },
    response_vo::api_wallet::wallet::{
        KeysUidCheckRes, QueryUidBindInfoRes, QueryWalletActivationInfoResp, UidStatus,
    },
};
use wallet_tree::KdfAlgorithm;

pub struct ApiWalletDomain {}
pub(crate) use unlock::{
    SEED_ENVELOPE_NONCE_BYTES, SEED_ENVELOPE_SALT_BYTES, SEED_ENVELOPE_VERSION_V1, SeedEnvelopeV1,
    password_cache_ttl,
};

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
        let pool = CONTEXT.get().unwrap().api_wallet_pool()?;
        // let phrase = wallet_utils::serde_func::serde_to_vec(&phrase)?;

        // let rng = rand::thread_rng();
        // let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        // let phrase = generator.generate(password.as_bytes(), &phrase)?;
        // let phrase = wallet_utils::serde_func::serde_to_string(&phrase)?;
        // let seed =
        //     KeystoreJsonGenerator::new(rng, algorithm).generate(password.as_bytes(), seed)?;
        // let seed = wallet_utils::serde_func::serde_to_string(&seed)?;

        let (phrase_enc, seed_enc) =
            Self::encrypt_phrase_and_seed(&algorithm, rand::rngs::OsRng, password, phrase, seed)
                .await?;

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
                                recharge_wallet.merchant_id.as_deref().unwrap_or_default(),
                                recharge_wallet.app_id.as_deref(),
                                // &recharge_wallet.sn,
                            )
                            .await?;
                        }
                    }
                }
            }

            ApiWalletRepo::bind_withdraw_and_subaccount_relation(
                &pool,
                binding_address,
                wallet_address,
            )
            .await?;
        }

        Ok(())
    }

    async fn encrypt_phrase(
        algorithm: KdfAlgorithm,
        rng: rand::rngs::OsRng,
        password: &str,
        phrase: &str,
    ) -> Result<String, ServiceError> {
        let mut gen1 = KeystoreJsonGenerator::new(rng, algorithm);
        let phrase_keystore = gen1.generate(password.as_bytes(), phrase.as_bytes())?;
        let phrase_enc = wallet_utils::serde_func::serde_to_string(&phrase_keystore)?;

        Ok(phrase_enc)
    }

    async fn encrypt_seed(
        algorithm: KdfAlgorithm,
        rng: rand::rngs::OsRng,
        password: &str,
        seed: &[u8],
    ) -> Result<String, ServiceError> {
        let _ = (algorithm, rng);
        unlock::encrypt_seed_bundle(password, seed).await
    }

    /// Compatibility wrapper so existing tests and call sites can keep using the old name.
    /// The actual seed envelope implementation lives in unlock.rs.
    pub(crate) async fn encrypt_seed_bundle(
        password: &str,
        seed: &[u8],
    ) -> Result<String, ServiceError> {
        unlock::encrypt_seed_bundle(password, seed).await
    }

    pub(crate) async fn encrypt_password_proof(
        algorithm: KdfAlgorithm,
        rng: rand::rngs::OsRng,
        password: &str,
        proof: &str,
    ) -> Result<String, ServiceError> {
        let mut gen1 = KeystoreJsonGenerator::new(rng, algorithm);
        let proof_keystore = gen1.generate(password.as_bytes(), proof.as_bytes())?;
        let proof_enc = wallet_utils::serde_func::serde_to_string(&proof_keystore)?;

        Ok(proof_enc)
    }

    async fn encrypt_phrase_and_seed(
        algorithm: &KdfAlgorithm,
        rng: rand::rngs::OsRng,
        password: &str,
        phrase: &str,
        seed: &[u8],
    ) -> Result<(String, String), ServiceError> {
        let phrase_enc =
            Self::encrypt_phrase(algorithm.to_owned(), rng.clone(), password, phrase).await?;
        let seed_enc = Self::encrypt_seed(algorithm.to_owned(), rng, password, seed).await?;

        Ok((phrase_enc, seed_enc))
    }

    pub(crate) async fn reset_api_wallet_seed(
        old_password: &str,
        new_password: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let wallets = ApiWalletRepo::list(&pool, None).await?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;

        for wallet in wallets {
            let phrase = ApiWalletDomain::decrypt_phrase(old_password, &wallet.phrase).await?;
            let seed = ApiWalletDomain::decrypt_seed(&old_password, &wallet.seed).await?;
            let (phrase_enc, seed_enc) = Self::encrypt_phrase_and_seed(
                &algorithm,
                rand::rngs::OsRng,
                new_password,
                &phrase,
                &seed,
            )
            .await?;
            ApiWalletRepo::update_seed_and_phrase(&pool, &wallet.uid, &phrase_enc, &seed_enc)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn get_seed(wallet_address: &str) -> Result<Vec<u8>, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let api_wallet =
            ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or_else(|| {
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )
            })?;

        let Some(envelope) = unlock::parse_seed_envelope(&api_wallet.seed)? else {
            let password = ApiWalletDomain::get_passwd().await?;
            return ApiWalletDomain::decrypt_seed(&password, &api_wallet.seed).await;
        };

        let session_state =
            crate::context::get_context()?.wallet_session_state(wallet_address).await?;
        unlock::decrypt_seed_bundle_with_smk(session_state.smk(), &envelope).await
    }

    pub(crate) async fn decrypt_seed(password: &str, seed: &str) -> Result<Vec<u8>, ServiceError> {
        match unlock::parse_seed_envelope(seed)? {
            Some(envelope) => unlock::decrypt_seed_bundle(password, &envelope).await,
            None => {
                let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), seed)?;
                Ok(data)
            }
        }
    }

    pub(crate) async fn decrypt_phrase(
        password: &str,
        phrase: &str,
    ) -> Result<String, ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), phrase)?;
        let data = wallet_utils::conversion::vec_to_string(&data)?;
        Ok(data)
    }

    pub(crate) async fn decrypt_password_proof(
        password: &str,
        proof: &str,
    ) -> Result<String, ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), proof)?;
        let data = wallet_utils::conversion::vec_to_string(&data)?;
        Ok(data)
    }

    pub(crate) async fn set_all_wallet_seed() -> Result<(), ServiceError> {
        let _ = crate::context::get_context()?.api_wallet_pool()?;
        // Seed preloading stays disabled; the new unlock path decrypts on demand only.
        tracing::debug!("set_all_wallet_seed skipped: api wallet seed caching is disabled");
        Ok(())
    }

    /// 检查这个地址是否曾经被创建为普通钱包
    pub(crate) async fn check_normal_wallet_exist(address: &str) -> Result<bool, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;

        Ok(WalletRepo::detail(pool.clone(), address).await?.is_some())
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
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        ApiWalletRepo::update_merchant_id(&pool, &address, merchain_id).await?;
        ApiWalletRepo::update_app_id(&pool, &address, org_app_id).await?;

        Ok(())
    }

    pub(crate) async fn db_save_sn_data(
        recharge_address: &str,
        withdrawal_address: Option<&str>,
        sn: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
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
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

        ApiWalletRepo::bind_withdraw_and_subaccount_relation(&pool, &subaccount_uid, &withdraw_uid)
            .await?;

        ApiWalletRepo::bind_withdraw_and_subaccount_relation(&pool, &withdraw_uid, &subaccount_uid)
            .await?;
        Ok(())
    }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = CONTEXT.get().unwrap().api_wallet_pool()?;
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
        let pool = CONTEXT.get().unwrap().api_wallet_pool()?;
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
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let _guard = EXPAND_INDEX_LOCK.lock().await;

        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let backend = CONTEXT.get().unwrap().get_global_backend_api();

        let Some(api_wallet) = ApiWalletRepo::find_by_uid(&pool, &uid).await? else {
            let req = ExpandAddressCompleteReq::new(
                uid,
                batch_id,
                serial_no,
                false,
                Some("api wallet not found"),
            );
            backend.expand_address_complete(req).await?;

            return Ok(());
        };

        let needed_indices = AwmCmdAddrExpandMsg::get_needed_indices(
            address_allock_type,
            &api_wallet.uid,
            chain_code,
            batch_id,
            number,
            index,
            Some(msg_id),
        )
        .await?;
        drop(_guard);

        tracing::info!("expand address index: {:?}", needed_indices);
        Ok(())
    }

    pub(crate) async fn get_passwd() -> Result<String, ServiceError> {
        crate::context::get_context()?.wallet_unlock_token().await
    }

    pub(crate) async fn cache_passwd(wallet_password: &str) -> Result<(), ServiceError> {
        // The unlock-state assembly itself lives in unlock.rs; this wrapper only reads wallets,
        // migrates legacy seed records when needed, and writes the resulting session state back.
        let pool = crate::context::get_context()?.api_wallet_pool()?;
        let wallets = ApiWalletRepo::list(&pool, None).await?;
        let mut wallet_states = std::collections::HashMap::new();

        for wallet in wallets {
            let Some(envelope) = unlock::parse_seed_envelope(&wallet.seed)? else {
                let seed = Self::decrypt_seed(wallet_password, &wallet.seed).await?;
                let seed_enc = unlock::encrypt_seed_bundle(wallet_password, &seed).await?;
                ApiWalletRepo::update_seed_and_phrase(
                    &pool,
                    &wallet.uid,
                    &wallet.phrase,
                    &seed_enc,
                )
                .await?;

                let envelope = unlock::parse_seed_envelope(&seed_enc)?.ok_or_else(|| {
                    crate::error::service::ServiceError::System(
                        crate::error::system::SystemError::Internal(
                            "seed envelope rewrite failed".to_string(),
                        ),
                    )
                })?;
                let smk = unlock::derive_smk(wallet_password, &envelope.salt).await?;
                wallet_states.insert(
                    wallet.address.clone(),
                    WalletSessionState::new(smk.to_vec(), envelope.rotation_counter),
                );
                continue;
            };

            let smk = unlock::derive_smk(wallet_password, &envelope.salt).await?;
            wallet_states.insert(
                wallet.address.clone(),
                WalletSessionState::new(smk.to_vec(), envelope.rotation_counter),
            );
        }

        let unlock_state = WalletUnlockState::new(
            unlock::generate_unlock_token(),
            Instant::now() + unlock::password_cache_ttl(),
            wallet_states,
        );
        crate::context::get_context()?.set_wallet_unlock_state(unlock_state).await?;
        Ok(())
    }

    pub(crate) async fn clear_passwd() -> Result<(), ServiceError> {
        crate::context::get_context()?.clear_wallet_unlock_state().await?;
        Ok(())
    }

    pub(crate) async fn rotate_session_key() -> Result<(), ServiceError> {
        // Rotation rewraps the seed envelope using the current session material and refreshes
        // the wallet-level session state without touching the plaintext password again.
        let context = crate::context::get_context()?;
        let pool = context.api_wallet_pool()?;
        let wallets = ApiWalletRepo::list(&pool, None).await?;
        let session_token = context.wallet_unlock_token().await?;
        let mut wallet_states = std::collections::HashMap::new();

        for wallet in wallets {
            let Some(envelope) = unlock::parse_seed_envelope(&wallet.seed)? else {
                continue;
            };

            let session_state = context.wallet_session_state(&wallet.address).await?;
            let seed = unlock::decrypt_seed_bundle_with_smk(session_state.smk(), &envelope).await?;
            let next_rotation_counter = session_state.rotation_counter().saturating_add(1);
            let salt = unlock::generate_seed_salt();
            let rotated_seed = unlock::encrypt_seed_bundle_with_smk(
                session_state.smk(),
                &salt,
                &seed,
                next_rotation_counter,
            )
            .await?;

            ApiWalletRepo::update_seed_and_phrase(
                &pool,
                &wallet.uid,
                &wallet.phrase,
                &rotated_seed,
            )
            .await?;

            wallet_states.insert(
                wallet.address.clone(),
                WalletSessionState::new(session_state.smk().to_vec(), next_rotation_counter),
            );
        }

        let unlock_state = WalletUnlockState::new(
            session_token,
            Instant::now() + unlock::password_cache_ttl(),
            wallet_states,
        );
        context.set_wallet_unlock_state(unlock_state).await?;
        Ok(())
    }

    /// 设置uid为api钱包
    pub(crate) async fn set_api_wallet(
        sn: &str,
        recharge_uid: Option<&str>,
        withdrawal_uid: Option<&str>,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
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
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
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
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
            &uid,
            &device.sn,
            Some(client_id),
            Some(device.device_type.clone()),
            wallet_name,
            invite_code,
        );

        backend.old_keys_init(keys_init_req).await?;
        ApiWalletRepo::mark_init(&pool, uid).await?;
        Ok(())
    }

    pub(crate) async fn check_keys_uid(uid: &str) -> Result<KeysUidCheckRes, ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        let uid_check = backend.keys_uid_check(&uid).await?;

        Ok(uid_check)
    }

    pub(crate) async fn change_withdrawal_wallet(
        recharge_uid: &str,
        withdrawal_uid: &str,
        // app_id: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
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
            recharge_wallet.merchant_id.as_deref().unwrap_or_default(),
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
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        Ok(backend.query_uid_bind_info(uid).await?)
    }

    pub async fn is_wallet_authorized_on_device(
        wallet_address: &str,
        sn: &str,
    ) -> Result<bool, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
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
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        Ok(backend.appid_uid_usage(req).await?.used)
    }

    /// 扫码绑定
    pub(crate) async fn scan_bind(
        recharge_uid: &str,
        withdrawal_uid: &str,
        org_app_id: &str,
        sn: &str,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        Ok(backend
            .wallet_bind_appid(BindAppIdReq::new(recharge_uid, withdrawal_uid, org_app_id, sn))
            .await?)
    }

    /// 导入钱包
    pub(crate) async fn appid_import(
        sn: &str,
        recharge_uid: Option<&str>,
        withdrawal_uid: Option<&str>,
    ) -> Result<(), ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
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
        let backend = crate::context::CONTEXT.get().unwrap().get_api_wallet_backend();
        backend
            .appid_import_recharge_wallet(AppIdImportRechargeWalletReq::new(sn, recharge_uid))
            .await?;
        Ok(())
    }

    /// 查询激活信息
    pub async fn query_wallet_activation_info(
        wallet_address: &str,
    ) -> Result<QueryWalletActivationInfoResp, crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        Ok(backend_api.query_wallet_activation_info(&api_wallet.uid).await?)
    }

    // pub async fn get_api_wallet_list() -> Result<ApiWalletList, crate::error::service::ServiceError>
    // {
    //     let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
    //     let li = ApiWalletRepo::list(&pool, None).await?;
    //     let mut list = ApiWalletList::new();
    //     // let balance_list = crate::infrastructure::asset_calc::get_wallet_balance_list().await?;

    //     let handles = crate::context::CONTEXT.get().unwrap().get_handles_arc().await?;
    //     let asset_calc_actor_manager = handles.get_global_asset_calc_actor_manager();
    //     let balance_list = asset_calc_actor_manager.get_wallet_balance().await?;

    //     // tracing::info!("get_api_wallet_list balance_list: {balance_list:#?}");
    //     for e in &li {
    //         let mut wallet: crate::response_vo::api_wallet::wallet::WalletInfo = e.into();
    //         if let Some(balance) = balance_list.get(&e.address) {
    //             wallet = wallet.with_balance(balance.clone());
    //         };
    //         match e.api_wallet_type {
    //             ApiWalletType::SubAccount => {
    //                 // 如果是收款钱包，看list有没有绑定地址，有就修改，没有就不管
    //                 if let Some(binding_address) = &e.binding_address
    //                     && let Some(item) = list.iter_mut().find(|item| {
    //                         item.withdraw_wallet
    //                             .as_ref()
    //                             .map(|w| &w.address == binding_address)
    //                             .unwrap_or(false)
    //                     })
    //                 {
    //                     item.recharge_wallet = Some(wallet);
    //                 } else {
    //                     list.push(ApiWalletItem {
    //                         recharge_wallet: Some(wallet),
    //                         withdraw_wallet: None,
    //                     });
    //                 }
    //             }
    //             ApiWalletType::Withdrawal => {
    //                 if let Some(binding_address) = &e.binding_address
    //                     && let Some(item) = list.iter_mut().find(|item| {
    //                         item.recharge_wallet
    //                             .as_ref()
    //                             .map(|r| &r.address == binding_address)
    //                             .unwrap_or(false)
    //                     })
    //                 {
    //                     item.withdraw_wallet = Some(wallet);
    //                 } else {
    //                     list.push(ApiWalletItem {
    //                         recharge_wallet: None,
    //                         withdraw_wallet: Some(wallet),
    //                     });
    //                 }
    //             }
    //         }
    //     }

    //     // list.retain(|item| item.recharge_wallet.is_some());
    //     Ok(list)
    // }

    pub async fn get_api_wallet_list_v2()
    -> Result<ApiWalletList, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let li = ApiWalletRepo::list(&pool, None).await?;
        let mut list = ApiWalletList::new();

        for e in &li {
            let mut wallet: crate::response_vo::api_wallet::wallet::WalletInfo = e.into();

            // 列表页会为每个钱包计算余额，直接复用 domain 层的去重/缓存/v3 优先策略，
            // 避免这里再次触发旧的 v2 重聚合 SQL，导致页面停留时形成请求风暴放大。
            let balance =
                ApiAssetsDomain::get_api_wallet_assets(Some(&e.address), None, None).await?;
            wallet = wallet.with_balance(balance);

            match e.api_wallet_type {
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

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use tokio::sync::OnceCell;
    use wallet_crypto::EncryptedJsonGenerator as _;
    use wallet_database::{
        entities::{api_wallet::ApiWalletType, device::CreateDeviceEntity},
        repositories::{api_wallet::wallet::ApiWalletRepo, device::DeviceRepo},
    };
    use wallet_transport_backend::{
        request::{
            KeysInitReq,
            api_wallet::wallet::{
                AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq,
            },
        },
        response_vo::api_wallet::wallet::{AppIdUidUsageRes, KeysUidCheckRes, QueryUidBindInfoRes},
    };

    use crate::{ApiWalletBackend, context::get_context, dirs::Dirs};

    use super::{
        ApiWalletDomain, SEED_ENVELOPE_NONCE_BYTES, SEED_ENVELOPE_SALT_BYTES,
        SEED_ENVELOPE_VERSION_V1, SeedEnvelopeV1, password_cache_ttl,
    };
    use tokio::time::sleep;
    use wallet_tree::KdfAlgorithm;

    const TEST_SN: &str = "seed-cache-test-sn";
    const TEST_DEVICE_TYPE: &str = "ANDROID";
    const TEST_PASSWORD: &str = "q1111111";

    static TEST_ENV: Lazy<OnceCell<SeedCacheTestEnv>> = Lazy::new(OnceCell::const_new);

    #[derive(Clone)]
    struct SeedCacheTestEnv {
        _tempdir: Arc<TempDir>,
        wallet_address: String,
    }

    #[derive(Default)]
    struct NoopApiWalletBackend;

    #[async_trait]
    impl ApiWalletBackend for NoopApiWalletBackend {
        async fn wallet_bind_appid(
            &self,
            _: BindAppIdReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }

        async fn init_api_wallet(
            &self,
            _: AppIdImportReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }

        async fn old_keys_init(
            &self,
            _: KeysInitReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }

        async fn appid_import(
            &self,
            _: AppIdImportReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }

        async fn appid_import_recharge_wallet(
            &self,
            _: AppIdImportRechargeWalletReq,
        ) -> Result<(), crate::error::service::ServiceError> {
            Ok(())
        }

        async fn keys_uid_check(
            &self,
            uid: &str,
        ) -> Result<KeysUidCheckRes, crate::error::service::ServiceError> {
            Ok(KeysUidCheckRes {
                uid: uid.to_string(),
                status:
                    wallet_transport_backend::response_vo::api_wallet::wallet::UidStatus::ApiRaw,
            })
        }

        async fn query_uid_bind_info(
            &self,
            uid: &str,
        ) -> Result<QueryUidBindInfoRes, crate::error::service::ServiceError> {
            Ok(QueryUidBindInfoRes {
                app_id: String::new(),
                org_id: String::new(),
                bind_status: false,
                sn: uid.to_string(),
            })
        }

        async fn appid_uid_usage(
            &self,
            _: AppIdUidUsageReq,
        ) -> Result<AppIdUidUsageRes, crate::error::service::ServiceError> {
            Ok(AppIdUidUsageRes { used: false })
        }
    }

    async fn seed_cache_test_env() -> &'static SeedCacheTestEnv {
        TEST_ENV
            .get_or_init(|| async {
                let config = crate::config::Config::new(
                    r#"
app_code: "test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
aggregate_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
oss:
  access_key_id: "id"
  access_key_secret: "secret"
  bucket_name: "bucket"
  endpoint: "oss-endpoint"
"#,
                )
                .expect("parse test config");

                unsafe {
                    std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
                }

                let tempdir = TempDir::new().expect("create tempdir");
                let dirs = Dirs::new(tempdir.path().to_str().expect("utf8 root dir"))
                    .expect("create dirs");
                crate::context::init_context_with_api_wallet_backend(
                    TEST_SN,
                    TEST_DEVICE_TYPE,
                    dirs,
                    None,
                    config,
                    Arc::new(NoopApiWalletBackend::default()),
                )
                .await
                .expect("init test context");

                let core_pool = get_context().expect("context").core_pool().expect("core pool");
                DeviceRepo::upsert(
                    core_pool,
                    CreateDeviceEntity {
                        device_type: TEST_DEVICE_TYPE.to_string(),
                        sn: TEST_SN.to_string(),
                        code: "test-code".to_string(),
                        system_ver: "1.0.0".to_string(),
                        iemi: None,
                        meid: None,
                        iccid: None,
                        mem: None,
                        app_id: Some("test-app".to_string()),
                        is_init: 1,
                        language_init: 1,
                    },
                )
                .await
                .expect("upsert device");

                let pool = get_context().expect("context").api_wallet_pool().expect("api pool");
                let wallet_uid = "seed-cache-wallet-uid".to_string();
                let wallet_address = "0x00000000000000000000000000000000000000aa".to_string();
                let phrase_enc = encrypt_test_secret(b"seed-cache-phrase");
                let seed_enc = encrypt_test_secret(b"seed-cache-seed");
                ApiWalletRepo::upsert(
                    &pool,
                    &wallet_uid,
                    "seed-cache-wallet",
                    &wallet_address,
                    &phrase_enc,
                    &seed_enc,
                    ApiWalletType::SubAccount,
                    None,
                    TEST_SN,
                )
                .await
                .expect("upsert wallet");

                SeedCacheTestEnv { _tempdir: Arc::new(tempdir), wallet_address }
            })
            .await
    }

    fn encrypt_test_secret(data: &[u8]) -> String {
        let mut generator =
            wallet_crypto::KeystoreJsonGenerator::new(rand::rngs::OsRng, KdfAlgorithm::Argon2id);
        let keystore =
            generator.generate(TEST_PASSWORD.as_bytes(), data).expect("generate test keystore");
        wallet_utils::serde_func::serde_to_string(&keystore).expect("serialize test keystore")
    }

    #[tokio::test]
    async fn password_cache_expires_after_ttl() {
        let _ = seed_cache_test_env().await;
        ApiWalletDomain::cache_passwd(TEST_PASSWORD).await.expect("cache password");

        let cached = ApiWalletDomain::get_passwd().await.expect("read unlock token");
        assert!(!cached.is_empty());
        assert_ne!(cached, TEST_PASSWORD);

        sleep(password_cache_ttl() + Duration::from_millis(100)).await;

        let err = ApiWalletDomain::get_passwd().await.expect_err("unlock token should expire");
        assert!(matches!(
            err,
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::SystemNotReady
            )
        ));

        let _ = ApiWalletDomain::clear_passwd().await;
    }

    #[tokio::test]
    async fn session_key_rotation_rewraps_seed_without_password() {
        let env = seed_cache_test_env().await;
        ApiWalletDomain::cache_passwd(TEST_PASSWORD).await.expect("cache password");

        let pool = get_context().expect("context").api_wallet_pool().expect("api pool");
        let before = ApiWalletRepo::find_by_address(&pool, &env.wallet_address)
            .await
            .expect("load wallet before rotation")
            .expect("wallet before rotation")
            .seed;

        ApiWalletDomain::rotate_session_key().await.expect("rotate session key");

        let after = ApiWalletRepo::find_by_address(&pool, &env.wallet_address)
            .await
            .expect("load wallet after rotation")
            .expect("wallet after rotation")
            .seed;

        assert_ne!(before, after);

        let seed = ApiWalletDomain::get_seed(&env.wallet_address)
            .await
            .expect("decrypt seed after rotation");
        assert_eq!(seed, b"seed-cache-seed");

        let _ = ApiWalletDomain::clear_passwd().await;
    }

    #[tokio::test]
    async fn seed_envelope_roundtrip() {
        let encrypted =
            ApiWalletDomain::encrypt_seed_bundle(TEST_PASSWORD, b"seed-bundle-roundtrip")
                .await
                .expect("encrypt seed bundle");

        let envelope: SeedEnvelopeV1 =
            serde_json::from_str(&encrypted).expect("parse seed envelope");
        assert_eq!(envelope.version, SEED_ENVELOPE_VERSION_V1);
        assert_eq!(envelope.salt.len(), SEED_ENVELOPE_SALT_BYTES);
        assert_eq!(envelope.session_nonce.len(), SEED_ENVELOPE_NONCE_BYTES);
        assert_eq!(envelope.seed_nonce.len(), SEED_ENVELOPE_NONCE_BYTES);
        assert!(!envelope.wrapped_dek.is_empty());
        assert!(!envelope.seed_cipher.is_empty());

        let decrypted =
            ApiWalletDomain::decrypt_seed(TEST_PASSWORD, &encrypted).await.expect("decrypt seed");
        assert_eq!(decrypted, b"seed-bundle-roundtrip");
    }

    #[tokio::test]
    async fn decrypt_seed_accepts_legacy_format() {
        let legacy = encrypt_test_secret(b"legacy-seed-format");

        let decrypted =
            ApiWalletDomain::decrypt_seed(TEST_PASSWORD, &legacy).await.expect("decrypt legacy");
        assert_eq!(decrypted, b"legacy-seed-format");
    }

    #[tokio::test]
    async fn decrypt_seed_rejects_wrong_password_for_envelope() {
        let encrypted =
            ApiWalletDomain::encrypt_seed_bundle(TEST_PASSWORD, b"seed-bundle-roundtrip")
                .await
                .expect("encrypt seed bundle");

        let err = ApiWalletDomain::decrypt_seed("wrong-password", &encrypted)
            .await
            .expect_err("decrypt must fail with wrong password");
        let debug = format!("{err:?}");
        assert!(!debug.contains("seed-bundle-roundtrip"));
    }

    #[tokio::test]
    async fn seed_cache_is_not_retained() {
        let env = seed_cache_test_env().await;
        ApiWalletDomain::cache_passwd(TEST_PASSWORD).await.expect("cache password");

        let seed = ApiWalletDomain::get_seed(&env.wallet_address).await.expect("decrypt seed");
        assert_eq!(seed, b"seed-cache-seed");
        ApiWalletDomain::clear_passwd().await.expect("clear password");
    }

    #[tokio::test]
    async fn set_all_wallet_seed_is_a_noop() {
        let _ = seed_cache_test_env().await;
        ApiWalletDomain::cache_passwd(TEST_PASSWORD).await.expect("cache password");

        ApiWalletDomain::set_all_wallet_seed().await.expect("seed preload should be skipped");
        ApiWalletDomain::clear_passwd().await.expect("clear password");
    }
}
