use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{api_wallet::ApiWalletRepo, device::DeviceRepo},
};
use wallet_transport_backend::request::{LanguageInitReq, api_wallet::wallet::BindAppIdReq};

use crate::{
    domain::{api_wallet::wallet::ApiWalletDomain, app::DeviceDomain, wallet::WalletDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, task::Tasks},
};

pub struct ApiWalletService {}

impl ApiWalletService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn create_wallet(
        self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let start = std::time::Instant::now();

        let password_validation_start = std::time::Instant::now();
        WalletDomain::validate_password(wallet_password).await?;
        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());

        let pool = crate::Context::get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(&pool).await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo { private_key: _, seed, address, phrase } =
            wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        let address = &address.to_string();

        if ApiWalletDomain::check_normal_wallet_exist(address).await? {
            return Err(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem,
            )
            .into());
        }

        tracing::debug!("Master key generation took: {:?}", master_key_start.elapsed());

        // let uid = wallet_utils::md5(&format!("{phrase}{salt}"));
        let pbkdf2_string_start = std::time::Instant::now();
        let uid = wallet_utils::pbkdf2_string(&format!("{phrase}{salt}"), salt, 100000, 32)?;

        // uid类型检查
        // TODO:
        // let backend = crate::Context::get_global_backend_api()?;
        // backend.keys_uid_check(&uid).await?;

        tracing::debug!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());
        let seed = seed.clone();

        let initialize_root_keystore_start = std::time::Instant::now();

        ApiWalletDomain::upsert_api_wallet(
            &uid,
            wallet_name,
            address,
            wallet_password,
            &phrase,
            &seed,
            api_wallet_type,
        )
        .await?;
        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );

        match api_wallet_type {
            ApiWalletType::SubAccount => {}
            ApiWalletType::Withdrawal => {
                ApiWalletDomain::create_account(
                    address,
                    wallet_password,
                    None,
                    account_name,
                    is_default_name,
                    1,
                    api_wallet_type,
                )
                .await?
            }
        }

        // let default_chain_list = tx.get_chain_list().await?;
        // let coins = tx.default_coin_list().await?;

        // let default_chain_list = default_chain_list
        //     .into_iter()
        //     .map(|chain| chain.chain_code)
        //     .collect::<Vec<String>>();
        // tracing::info!("coins: {:?}", coins);
        // let account_creation_start = std::time::Instant::now();
        // let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        // let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();

        // let mut address_init_task_data = AddressBatchInitReq(Vec::new());
        // for account_id in account_ids {
        //     let account_index_map =
        //         wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;

        //     ChainDomain::init_chains_api_assets(
        //         tx,
        //         &coins,
        //         &mut req,
        //         &mut address_init_task_data,
        //         &mut subkeys,
        //         &default_chain_list,
        //         &seed,
        //         &account_index_map,
        //         None,
        //         &uid,
        //         address,
        //         account_name,
        //         is_default_name,
        //     )
        //     .await?;
        // }
        // tracing::info!(
        //     "Account creation and subkey generation took: {:?}",
        //     account_creation_start.elapsed()
        // );

        // let child_keystore_start = std::time::Instant::now();
        // let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        // let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        // let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;

        // KeystoreApi::initialize_child_keystores(
        //     wallet_tree,
        //     subkeys,
        //     dirs.get_subs_dir(address)?,
        //     wallet_password,
        //     algorithm,
        // )?;
        // tracing::debug!(
        //     "Child keystore initialization took: {:?}",
        //     child_keystore_start.elapsed()
        // );

        // let task = Task::Common(CommonTask::QueryCoinPrice(req));
        // Tasks::new().push(task).send().await?;
        // tx.update_uid(Some(&uid)).await?;

        let client_id = DeviceDomain::client_id_by_device(&device)?;

        let language_req = {
            let config = crate::app_state::APP_STATE.read().await;
            LanguageInitReq::new(&client_id, config.language())
        };

        let keys_init_req = wallet_transport_backend::request::KeysInitReq::new(
            &uid,
            &device.sn,
            Some(client_id),
            Some(device.device_type),
            wallet_name,
            invite_code,
        );
        let keys_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::KEYS_V2_INIT,
            &keys_init_req,
        )?;

        let language_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::LANGUAGE_INIT,
            &language_req,
        )?;

        // let address_init_task_data = BackendApiTaskData::new(
        //     wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
        //     &address_init_task_data,
        // )?;
        Tasks::new()
            .push(BackendApiTask::BackendApi(keys_init_task_data))
            .push(BackendApiTask::BackendApi(language_init_task_data))
            //     .push(Task::BackendApi(BackendApiTask::BackendApi(
            //         address_init_task_data,
            //     )))
            .send()
            .await?;

        tracing::debug!("cose time: {}", start.elapsed().as_millis());
        Ok(())
    }

    pub async fn import_wallet(
        self,
        language_code: u8,
        phrase: &str,
        salt: &str,
        wallet_name: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        invite_code: Option<String>,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let start = std::time::Instant::now();

        let password_validation_start = std::time::Instant::now();
        WalletDomain::validate_password(wallet_password).await?;
        tracing::debug!("Password validation took: {:?}", password_validation_start.elapsed());

        let pool = crate::Context::get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(&pool).await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        let dirs = crate::manager::Context::get_global_dirs()?;

        let master_key_start = std::time::Instant::now();
        let wallet_tree::api::RootInfo { private_key: _, seed, address, phrase } =
            wallet_tree::api::KeystoreApi::generate_master_key_info(language_code, phrase, salt)?;
        let address = &address.to_string();

        if ApiWalletDomain::check_normal_wallet_exist(address).await? {
            return Err(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::MnemonicAlreadyImportedIntoNormalWalletSystem,
            )
            .into());
        }

        tracing::debug!("Master key generation took: {:?}", master_key_start.elapsed());

        // let uid = wallet_utils::md5(&format!("{phrase}{salt}"));
        let pbkdf2_string_start = std::time::Instant::now();
        let uid = wallet_utils::pbkdf2_string(&format!("{phrase}{salt}"), salt, 100000, 32)?;
        tracing::debug!("Pbkdf2 string took: {:?}", pbkdf2_string_start.elapsed());
        let seed = seed.clone();

        let initialize_root_keystore_start = std::time::Instant::now();

        ApiWalletDomain::upsert_api_wallet(
            &uid,
            wallet_name,
            address,
            wallet_password,
            &phrase,
            &seed,
            api_wallet_type,
        )
        .await?;
        tracing::debug!(
            "Initialize root keystore took: {:?}",
            initialize_root_keystore_start.elapsed()
        );

        Ok(())
    }

    pub async fn bind_merchant(
        self,
        org_app_id: &str,
        merchain_id: &str,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), crate::ServiceError> {
        ApiWalletDomain::bind_uid(recharge_uid, merchain_id, org_app_id).await?;
        ApiWalletDomain::bind_uid(withdrawal_uid, merchain_id, org_app_id).await?;

        let backend = crate::Context::get_global_backend_api()?;
        backend
            .wallet_bind_appid(&BindAppIdReq::new(recharge_uid, withdrawal_uid, org_app_id))
            .await?;
        Ok(())
    }

    pub async fn unbind_merchant(
        self,
        recharge_uid: &str,
        withdrawal_uid: &str,
    ) -> Result<(), crate::ServiceError> {
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
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        ApiWalletRepo::edit_name(&pool, address, name).await?;
        Ok(())
    }

    pub async fn set_passwd_cache(self, wallet_password: &str) -> Result<(), crate::ServiceError> {
        ApiWalletDomain::set_passwd(wallet_password).await?;
        Ok(())
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
