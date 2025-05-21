use wallet_database::{
    dao::multisig_member::MultisigMemberDaoV1,
    repositories::{
        account::AccountRepoTrait, chain::ChainRepoTrait, coin::CoinRepoTrait,
        device::DeviceRepoTrait, wallet::WalletRepoTrait, ResourcesRepo, TransactionTrait as _,
    },
};
use wallet_transport_backend::{
    consts::endpoint,
    request::{DeviceDeleteReq, LanguageInitReq, TokenQueryPriceReq},
};
use wallet_tree::{api::KeystoreApi, file_ops::RootData};
use wallet_types::constant::chain_code;

use crate::{
    domain::{
        self,
        account::AccountDomain,
        app::{config::ConfigDomain, DeviceDomain},
        assets::AssetsDomain,
        chain::ChainDomain,
        coin::CoinDomain,
        multisig::MultisigDomain,
        permission::PermissionDomain,
        wallet::WalletDomain,
    },
    infrastructure::task_queue::{
        BackendApiTask, BackendApiTaskData, CommonTask, RecoverDataBody, Task, Tasks,
    },
    response_vo::{
        account::BalanceInfo,
        chain::ChainCodeAndName,
        wallet::{CreateWalletRes, GeneratePhraseRes, QueryPhraseRes},
    },
};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Export {
    chain_code: String,
    derivation_path: String,
    address_type: Option<String>,
}

pub struct WalletService {
    pub repo: ResourcesRepo,
    wallet_domain: WalletDomain,
    assets_domain: AssetsDomain,
    coin_domain: CoinDomain,
}

impl WalletService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            wallet_domain: WalletDomain::new(),
            assets_domain: AssetsDomain::new(),
            coin_domain: CoinDomain::new(),
        }
    }

    pub(crate) async fn encrypt_password(
        mut self,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let Some(device) = self.repo.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        let encrypted_password = WalletDomain::encrypt_password(password, &device.sn)?;
        Ok(encrypted_password)
    }

    pub(crate) async fn validate_password(self, password: &str) -> Result<(), crate::ServiceError> {
        WalletDomain::validate_password(password).await?;
        Ok(())
    }

    pub(crate) async fn switch_wallet(
        self,
        wallet_address: &str,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let wallet = tx.update_wallet_update_at(wallet_address).await?;

        if let Some(wallet) = wallet {
            tx.update_uid(Some(&wallet.uid)).await?;
            let device = tx
                .get_device_info()
                .await?
                .ok_or(crate::BusinessError::Device(
                    crate::DeviceError::Uninitialized,
                ))?;
            let config = crate::app_state::APP_STATE.read().await;
            let language = config.language();

            let client_id = domain::app::DeviceDomain::client_id_by_device(&device)?;
            let language_req = wallet_transport_backend::request::LanguageInitReq {
                client_id,
                lan: language.to_string(),
            };
            let language_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
                &language_req,
            )?;
            Tasks::new()
                .push(Task::BackendApi(BackendApiTask::BackendApi(
                    language_init_task_data,
                )))
                .send()
                .await?;
        }

        Ok(())
    }

    pub async fn edit_wallet_name(
        self,
        wallet_name: &str,
        wallet_address: &str,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let wallet_list = tx.edit_wallet_name(wallet_address, wallet_name).await?;

        let device = tx.get_device_info().await?;
        if let Some(device) = &device {
            let client_id = domain::app::DeviceDomain::client_id_by_device(device)?;

            for wallet in wallet_list {
                let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
                    &wallet.uid,
                    &device.sn,
                    Some(client_id.clone()),
                    device.app_id.clone(),
                    Some(device.device_type.clone()),
                    wallet_name,
                    None,
                );
                let keys_init_task_data = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::KEYS_INIT,
                    &keys_init_req,
                )?;
                Tasks::new()
                    .push(Task::BackendApi(BackendApiTask::BackendApi(
                        keys_init_task_data,
                    )))
                    .send()
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn import_derivation_path(
        self,
        path: &str,
        wallet_address: &str,
        wallet_password: &str,
        account_name: &str,
        is_default_name: bool,
    ) -> Result<crate::response_vo::wallet::ImportDerivationPathRes, crate::ServiceError> {
        let mut tx = self.repo;

        WalletDomain::validate_password(wallet_password).await?;
        let dirs = crate::manager::Context::get_global_dirs()?;
        let mut buf = String::new();
        wallet_utils::file_func::read(&mut buf, path)?;

        let exports: Vec<Export> = wallet_utils::serde_func::serde_from_str(&buf)?;
        let seed = WalletDomain::get_seed(dirs, wallet_address, wallet_password).await?;

        let wallet = tx
            .wallet_detail_by_address(wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();
        let mut accounts = Vec::new();
        let mut address_init_task_data = Vec::new();
        for data in exports {
            let hd_path = wallet_chain_instance::derivation_path::get_account_hd_path_from_path(
                &data.derivation_path,
            )?;
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(hd_path.get_account_id()?)?;
            let Some(chain) = tx.detail_with_node(&data.chain_code).await? else {
                continue;
            };
            let instance = wallet_chain_instance::instance::ChainObject::new(
                &data.chain_code,
                data.address_type,
                chain.network.as_str().into(),
            )?;

            let (account, _, task_data) = AccountDomain::create_account(
                &mut tx,
                &seed,
                &instance,
                Some(&data.derivation_path),
                &account_index_map,
                &wallet.uid,
                wallet_address,
                account_name,
                is_default_name,
            )
            .await?;
            address_init_task_data.push(task_data);

            let keypair = instance
                .gen_keypair_with_index_address_type(&seed, account_index_map.input_index)
                .map_err(|e| crate::SystemError::Service(e.to_string()))?;
            let pk = keypair.private_key_bytes()?;
            let subkey = wallet_tree::file_ops::BulkSubkey::new(
                account_index_map.clone(),
                &account.address,
                &data.chain_code,
                &data.derivation_path,
                pk,
            );
            subkeys.push(subkey);
            accounts.push(account.address);
        }
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        KeystoreApi::initialize_child_keystores(
            wallet_tree,
            subkeys,
            dirs.get_subs_dir(wallet_address)?,
            wallet_password,
            algorithm,
        )?;

        for task in address_init_task_data {
            Tasks::new()
                .push(Task::BackendApi(BackendApiTask::BackendApi(task)))
                .send()
                .await?;
        }
        Ok(crate::response_vo::wallet::ImportDerivationPathRes { accounts })
    }

    pub async fn export_derivation_path(
        &mut self,
        wallet_address: &str,
    ) -> Result<crate::response_vo::wallet::ExportDerivationPathRes, crate::ServiceError> {
        let tx = &mut self.repo;
        let dirs = crate::manager::Context::get_global_dirs()?;
        let account_list = tx
            .get_account_list_by_wallet_address_and_account_id(Some(wallet_address), None)
            .await?;
        let mut derivation_paths = Vec::new();
        for account in account_list.into_iter() {
            let address_type = account.address_type();
            let export = Export {
                chain_code: account.chain_code,
                derivation_path: account.derivation_path,
                address_type,
            };
            derivation_paths.push(export);
        }

        let json = wallet_utils::serde_func::serde_to_string(&derivation_paths)?;
        let path = dirs.get_export_dir().join(wallet_address);
        wallet_utils::file_func::write(&json, &path)?;

        Ok(crate::response_vo::wallet::ExportDerivationPathRes {
            file_path: path.to_string_lossy().to_string(),
        })
    }

    pub async fn create_wallet(
        &mut self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
    ) -> Result<CreateWalletRes, crate::ServiceError> {
        let start = std::time::Instant::now();
        let tx = &mut self.repo;

        let password_validation_start = std::time::Instant::now();
        WalletDomain::validate_password(wallet_password).await?;
        tracing::info!(
            "Password validation took: {:?}",
            password_validation_start.elapsed()
        );

        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        let dirs = crate::manager::Context::get_global_dirs()?;

        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo {
            private_key: _,
            seed,
            address,
            phrase,
        } = wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        tracing::info!(
            "Master key generation took: {:?}",
            master_key_start.elapsed()
        );

        // let uid = wallet_utils::md5(&format!("{phrase}{salt}"));
        let pbkdf2_string_start = std::time::Instant::now();
        let uid = wallet_utils::pbkdf2_string(&format!("{phrase}{salt}"), salt, 100000, 32)?;
        tracing::info!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());
        let address = &address.to_string();
        let seed = seed.clone();

        // 检查钱包状态
        let account_ids = self
            .wallet_domain
            .restart_existing_wallet(tx, address)
            .await?;
        let storage_path = dirs.get_root_dir(address)?;
        wallet_utils::file_func::recreate_dir_all(&storage_path)?;

        let wallet_tree_start = std::time::Instant::now();
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        tracing::info!(
            "Wallet tree strategy retrieval took: {:?}",
            wallet_tree_start.elapsed()
        );

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        let initialize_root_keystore_start = std::time::Instant::now();
        wallet_tree::api::KeystoreApi::initialize_root_keystore(
            wallet_tree,
            address,
            // &private_key,
            RootData::new(&phrase, &seed),
            &storage_path,
            wallet_password,
            algorithm,
        )?;
        tracing::info!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );
        tx.upsert_wallet(address, &uid, wallet_name).await?;
        let default_chain_list = tx.get_chain_list().await?;
        let coins = tx.default_coin_list().await?;
        let default_chain_list = default_chain_list
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect::<Vec<String>>();

        let account_creation_start = std::time::Instant::now();
        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();

        let mut address_init_task_data = Vec::new();
        for account_id in account_ids {
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;

            ChainDomain::init_chains_assets(
                tx,
                &coins,
                &mut req,
                &mut address_init_task_data,
                &mut subkeys,
                &default_chain_list,
                &seed,
                &account_index_map,
                None,
                &uid,
                address,
                account_name,
                is_default_name,
            )
            .await?;
        }
        tracing::info!(
            "Account creation and subkey generation took: {:?}",
            account_creation_start.elapsed()
        );

        let child_keystore_start = std::time::Instant::now();
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;

        // 波场的地址
        let tron_address = subkeys
            .iter()
            .find(|s| s.chain_code == chain_code::TRON)
            .map(|s| s.address.clone());

        KeystoreApi::initialize_child_keystores(
            wallet_tree,
            subkeys,
            dirs.get_subs_dir(address)?,
            wallet_password,
            algorithm,
        )?;
        tracing::info!(
            "Child keystore initialization took: {:?}",
            child_keystore_start.elapsed()
        );

        let task = Task::Common(CommonTask::QueryCoinPrice(req));
        Tasks::new().push(task).send().await?;
        tx.update_uid(Some(&uid)).await?;

        let client_id = domain::app::DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
            &uid,
            &device.sn,
            Some(client_id),
            device.app_id,
            Some(device.device_type),
            wallet_name,
            invite_code,
        );
        let keys_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::KEYS_INIT,
            &keys_init_req,
        )?;

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;

        let uids = tx
            .uid_list()
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();
        let device_delete_req = DeviceDeleteReq::new(&device.sn, &uids);

        let device_delete_task_data =
            BackendApiTaskData::new(endpoint::DEVICE_DELETE, &device_delete_req)?;

        let device_bind_address_task_data =
            domain::app::DeviceDomain::gen_device_bind_address_task_data().await?;

        // 恢复多签账号、多签队列
        let mut recover_data = RecoverDataBody::new(&uid);
        if let Some(tron_address) = tron_address {
            recover_data.tron_address = Some(tron_address);
        };

        Tasks::new()
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                keys_init_task_data,
            )))
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                language_init_task_data,
            )))
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                device_delete_task_data,
            )))
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                device_bind_address_task_data,
            )))
            .push(Task::Common(CommonTask::RecoverMultisigAccountData(
                recover_data,
            )))
            .send()
            .await?;

        for task in address_init_task_data {
            Tasks::new()
                .push(Task::BackendApi(BackendApiTask::BackendApi(task)))
                .send()
                .await?;
        }

        tracing::info!("cose time: {}", start.elapsed().as_millis());
        Ok(CreateWalletRes {
            address: address.to_string(),
        })
    }

    pub async fn get_phrase(
        &mut self,
        wallet_address: &str,
        password: &str,
    ) -> Result<crate::response_vo::wallet::GetPhraseRes, crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        let root_dir = dirs.get_root_dir(wallet_address)?;

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let phrase = wallet_tree::api::KeystoreApi::load_phrase(
            &*wallet_tree,
            &root_dir,
            wallet_address,
            password,
        )?;
        Ok(crate::response_vo::wallet::GetPhraseRes { phrase })
    }

    pub(crate) fn generate_phrase(
        &self,
        language_code: u8,
        count: usize,
    ) -> Result<GeneratePhraseRes, crate::ServiceError> {
        let lang = wallet_core::language::Language::from_u8(language_code)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        let phrases = lang
            .gen_phrase(count)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        Ok(GeneratePhraseRes { phrases })
    }

    pub(crate) fn query_phrases(
        &self,
        language_code: u8,
        keyword: &str,
        mode: u8,
    ) -> Result<QueryPhraseRes, crate::ServiceError> {
        let wordlist_wrapper = wallet_core::language::WordlistWrapper::new(language_code)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;
        let mode = wallet_core::language::QueryMode::from_u8(mode)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        let phrases = wordlist_wrapper.query_phrase(keyword, mode);

        Ok(QueryPhraseRes { phrases })
    }

    pub(crate) fn exact_query_phrase(
        &self,
        language_code: u8,
        phrases: Vec<&str>,
    ) -> Result<Vec<String>, crate::ServiceError> {
        let wordlist_wrapper = wallet_core::language::WordlistWrapper::new(language_code)
            .map_err(|e| crate::SystemError::Service(e.to_string()))?;
        let res = phrases
            .iter()
            .map(|phrase| {
                wordlist_wrapper
                    .exact_query_phrase(phrase)
                    .unwrap_or_default()
            })
            .collect();

        Ok(res)
    }

    pub async fn get_wallet_list(
        &mut self,
        wallet_address: Option<String>,
        chain_code: Option<String>,
        account_id: Option<u32>,
    ) -> Result<Vec<crate::response_vo::wallet::WalletInfo>, crate::ServiceError> {
        let tx = &mut self.repo;
        let chains = tx.get_chain_list().await?;
        let chain_codes = if let Some(chain_code) = chain_code {
            vec![chain_code]
        } else {
            chains
                .iter()
                .map(|chain| chain.chain_code.clone())
                .collect()
        };

        let chains: ChainCodeAndName = chains.into();

        let token_currencies = self.coin_domain.get_token_currencies_v2(tx).await?;
        // let service = Service::default();
        let wallet_list = if let Some(wallet_address) = &wallet_address {
            let wallet = tx
                .wallet_detail_by_address(wallet_address)
                .await?
                .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;
            vec![wallet]
        } else {
            tx.wallet_list().await?
        };
        let mut res = Vec::new();
        for wallet_info in wallet_list {
            let list = tx
                .account_list_by_wallet_address_and_chain_code(
                    Some(&wallet_info.address),
                    chain_codes.clone(),
                    account_id,
                )
                .await?;
            let mut account_list = token_currencies
                .calculate_account_infos(list, &chains)
                .await?;
            // let mut account_cal_list = std::collections::HashMap::new();
            let mut wallet_assets = BalanceInfo::new_without_amount().await?;
            for account in account_list.iter_mut() {
                let mut account_assets_entity = self
                    .assets_domain
                    .get_account_assets_entity(
                        tx,
                        account.account_id,
                        &wallet_info.address,
                        chain_codes.clone(),
                        None,
                    )
                    .await?;

                let account_total_assets = token_currencies
                    .calculate_account_total_assets(&mut account_assets_entity)
                    .await?;
                let fiat_value = account_total_assets.fiat_value;
                let amount = account_total_assets.amount;
                account.balance.fiat_add(fiat_value);
                account.balance.amount_add(amount);
                wallet_assets.fiat_add(fiat_value);
                wallet_assets.amount_add(amount);
            }

            res.push(crate::response_vo::wallet::WalletInfo {
                address: wallet_info.address,
                uid: wallet_info.uid,
                name: wallet_info.name,
                balance: wallet_assets,
                created_at: wallet_info.created_at,
                updated_at: wallet_info.updated_at,
                account_list,
            });
        }

        Ok(res)
    }

    pub async fn logic_delete(self, address: &str) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        let wallet = tx.wallet_detail_by_address(address).await?;
        WalletRepoTrait::reset(&mut tx, address).await?;
        AccountRepoTrait::reset(&mut tx, address).await?;
        let latest_wallet = tx.wallet_latest().await?;

        let rest_uids = tx
            .uid_list()
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();

        let uid = if let Some(latest_wallet) = latest_wallet {
            Some(latest_wallet.uid)
        } else {
            None
        };
        tx.update_uid(uid.as_deref()).await?;
        let device = tx.get_device_info().await?;

        tx.commit_transaction().await?;
        let pool = crate::Context::get_global_sqlite_pool()?;

        if let Some(device) = device
            && let Some(wallet) = wallet
        {
            let members = MultisigMemberDaoV1::list_by_uid(&wallet.uid, &*pool)
                .await
                .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?;
            for member in members.0 {
                MultisigDomain::logic_delete_account(&member.account_id, pool.clone()).await?;
            }

            let req = DeviceDeleteReq::new(&device.sn, &rest_uids);

            let task = Task::BackendApi(BackendApiTask::BackendApi(BackendApiTaskData::new(
                endpoint::DEVICE_DELETE,
                &req,
            )?));
            Tasks::new().push(task).send().await?;
        };

        Ok(())
    }

    pub async fn physical_delete(self, address: &str) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;

        tx.begin_transaction().await?;
        let wallet = tx.wallet_detail_by_address(address).await?;
        WalletRepoTrait::physical_delete(&mut tx, &[address]).await?;
        let accounts = AccountRepoTrait::physical_delete_all(&mut tx, &[address]).await?;
        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        let dirs = crate::manager::Context::get_global_dirs()?;
        let wallet_dir = dirs.get_wallet_dir(Some(address));
        wallet_utils::file_func::remove_dir_all(wallet_dir)?;

        let latest_wallet = tx.wallet_latest().await?;

        let rest_uids = tx
            .uid_list()
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();

        let uid = if let Some(latest_wallet) = latest_wallet {
            Some(latest_wallet.uid)
        } else {
            KeystoreApi::remove_verify_file(&dirs.root_dir)?;
            tx.update_password(None).await?;
            None
        };
        tx.update_uid(uid.as_deref()).await?;
        tx.commit_transaction().await?;
        let pool = crate::Context::get_global_sqlite_pool()?;

        if let Some(wallet) = wallet {
            let req = DeviceDeleteReq::new(&device.sn, &rest_uids);
            let device_delete_task = Task::BackendApi(BackendApiTask::BackendApi(
                BackendApiTaskData::new(endpoint::DEVICE_DELETE, &req)?,
            ));

            let members = MultisigMemberDaoV1::list_by_uid(&wallet.uid, &*pool)
                .await
                .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?;

            let multisig_accounts =
                MultisigDomain::physical_delete_wallet_account(members, &wallet.uid, pool.clone())
                    .await?;

            let device_unbind_address_task = DeviceDomain::gen_device_unbind_all_address_task_data(
                &accounts,
                multisig_accounts,
                &device.sn,
            )
            .await?;

            let device_unbind_address_task =
                Task::BackendApi(BackendApiTask::BackendApi(device_unbind_address_task));
            Tasks::new()
                .push(device_delete_task)
                .push(device_unbind_address_task)
                .send()
                .await?;
        };

        // find tron address and del permission
        let tron_address = accounts.iter().find(|a| a.chain_code == chain_code::TRON);
        tracing::warn!("tron address = {:?}", tron_address);
        if let Some(address) = tron_address {
            PermissionDomain::delete_by_address(&pool, &address.address).await?;
        }

        for uid in rest_uids {
            let body = RecoverDataBody::new(&uid);

            Tasks::new()
                .push(Task::Common(CommonTask::RecoverMultisigAccountData(body)))
                .send()
                .await?;
        }
        Ok(())
    }

    pub async fn logic_reset(self) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        tx.begin_transaction().await?;
        let device = tx.get_device_info().await?;

        WalletRepoTrait::reset_all_wallet(&mut tx).await?;
        AccountRepoTrait::reset_all_account(&mut tx).await?;

        let dirs = crate::manager::Context::get_global_dirs()?;
        let wallet_dir = dirs.get_wallet_dir(None);
        wallet_utils::file_func::remove_dir_all(wallet_dir)?;
        tx.commit_transaction().await?;

        if let Some(device) = &device {
            let req = DeviceDeleteReq::new(&device.sn, &[]);

            let task = Task::BackendApi(BackendApiTask::BackendApi(BackendApiTaskData::new(
                endpoint::DEVICE_DELETE,
                &req,
            )?));
            Tasks::new().push(task).send().await?;
        };

        Ok(())
    }

    pub async fn physical_reset(self) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let mut tx = self.repo;
        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        tx.update_password(None).await?;

        WalletRepoTrait::physical_delete_all(&mut tx).await?;
        // 删除所有mqtt相关的任务
        // TaskQueueRepoTrait::delete_all(&mut tx, 2).await?;
        let accounts = AccountRepoTrait::physical_delete_all(&mut tx, &[]).await?;

        let req = DeviceDeleteReq::new(&device.sn, &[]);
        let device_delete_task = BackendApiTaskData::new(endpoint::DEVICE_DELETE, &req)?;
        let multisig_accounts = MultisigDomain::physical_delete_all_account(pool).await?;

        let device_unbind_address_task = DeviceDomain::gen_device_unbind_all_address_task_data(
            &accounts,
            multisig_accounts,
            &device.sn,
        )
        .await?;

        Tasks::new()
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                device_delete_task,
            )))
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                device_unbind_address_task,
            )))
            .send()
            .await?;

        let dirs = crate::manager::Context::get_global_dirs()?;
        let wallet_dir = dirs.get_wallet_dir(None);
        wallet_utils::file_func::remove_dir_all(&wallet_dir)?;
        wallet_utils::file_func::create_dir_all(wallet_dir)?;
        let file_name = "verify";
        let file_path = dirs.root_dir.join(file_name);
        wallet_utils::file_func::remove_file(file_path)?;
        Ok(())
    }

    pub async fn recover_multisig_data(
        self,
        _wallet_address: &str,
    ) -> Result<(), crate::ServiceError> {
        // 在创建钱包时，skd已经在任务里面添加了task 来恢复，这里没有必要给到前端一个接口再去执行一遍重复的逻辑
        // let mut tx = self.repo;
        // MultisigDomain::recover_multisig_account_and_queue_data(&mut tx, wallet_address).await?;

        Ok(())
    }

    pub async fn upgrade_algorithm(&self, password: &str) -> Result<(), crate::ServiceError> {
        WalletDomain::upgrade_algorithm(password).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    fn _uid(phrase: &str, salt: &str) -> String {
        let uid = format!("{phrase}{salt}");
        wallet_utils::md5(&uid)
    }

    fn uid_pbkdf2(phrase: &str, salt: &str) -> String {
        let uid = format!("{phrase}{salt}");
        wallet_utils::pbkdf2_string(&uid, salt, 100000, 32).unwrap()
    }

    #[tokio::test]
    async fn test_reset_root() {
        // let phrase =
        //     "chuckle practice chicken permit swarm giant improve absurd melt kitchen oppose scrub";
        // let phrase = "arrest hover fury mercy slim answer hospital area morning student riot deal";
        // let phrase = "spoil first width hat submit inflict impact quantum love funny warrior spike";
        // let phrase = "fetch bronze forward wish only gentle picture noise vocal essay devote steel";

        let phrase =
            "will match face problem tongue fortune rebuild stool moon assist virtual lounge";
        // let phrase =
        //     "drum planet ugly present absorb chair simple shiver honey object captain unable";
        // let phrase = "loan tiny planet lucky rigid clip coil recall praise obvious debris dilemma";
        // let phrase = "divorce word join around degree mother quiz math just custom lunar angle";
        // let phrase = "nose bird celery bread slice hero black session tonight winner pitch foot";
        // let phrase = "fan swamp loop mesh enact tennis priority artefact canal hour skull joy";

        let salt = "12345678";
        // let salt = "1234qwer";
        let uid_md5 = _uid(phrase, salt);
        let uid = uid_pbkdf2(phrase, salt);

        println!("uid_md5: {}", uid_md5);
        println!("uid: {}", uid);
    }
}
