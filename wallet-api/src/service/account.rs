use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{account::AccountEntity, assets::AssetsId, wallet::WalletEntity},
    repositories::{
        account::AccountRepoTrait, assets::AssetsRepoTrait, chain::ChainRepoTrait,
        coin::CoinRepoTrait, device::DeviceRepoTrait, wallet::WalletRepoTrait, ResourcesRepo,
    },
};
use wallet_transport_backend::request::TokenQueryPriceReq;
use wallet_tree::api::KeystoreApi;
use wallet_types::chain::{
    address::r#type::{AddressType, BTC_ADDRESS_TYPES},
    chain::ChainCode,
};
use wallet_utils::address::AccountIndexMap;

use crate::{
    domain::{self, account::AccountDomain, app::config::ConfigDomain, wallet::WalletDomain},
    infrastructure::task_queue::{BackendApiTask, CommonTask, Task, Tasks},
    response_vo::account::DerivedAddressesList,
};

pub struct AccountService {
    pub repo: ResourcesRepo,
    pub wallet_domain: WalletDomain,
    // keystore: wallet_keystore::Keystore
}

impl AccountService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            wallet_domain: WalletDomain::new(),
        }
    }

    pub(crate) async fn switch_account(
        self,
        _wallet_address: &str,
        _account_id: u32,
    ) -> Result<(), crate::ServiceError> {
        // let pool = crate::manager::Context::get_global_sqlite_pool()?;
        // let mut tx = self.repo;
        // let accounts = tx
        //     .get_account_list_by_wallet_address_and_account_id(
        //         Some(wallet_address),
        //         Some(account_id),
        //     )
        //     .await?;

        // let accounts = accounts
        //     .into_iter()
        //     .map(|account| account.address)
        //     .collect();
        // let regular_assets_list =
        //     AssetsRepoTrait::get_coin_assets_in_address_all_status(&mut tx, accounts).await?;

        // let multisig_accounts = domain::multisig::MultisigDomain::list(&pool).await?;

        // let multisig_accounts = multisig_accounts
        //     .into_iter()
        //     .map(|account| account.address)
        //     .collect();
        // let multisig_assets_list =
        //     AssetsRepoTrait::get_coin_assets_in_address_all_status(&mut tx, multisig_accounts)
        //         .await?;

        // Sync multisig assets status with regular assets
        // for multisig_asset in multisig_assets_list.iter() {
        //     if let Some(regular_asset) = regular_assets_list
        //         .iter()
        //         .find(|&ra| ra.symbol == multisig_asset.symbol)
        //     {
        //         AssetsRepoTrait::update_status(
        //             &mut tx,
        //             &multisig_asset.chain_code,
        //             &multisig_asset.symbol,
        //             multisig_asset.token_address(),
        //             regular_asset.status,
        //         )
        //         .await?;
        //     }
        // }

        Ok(())
    }

    pub async fn create_account(
        self,
        wallet_address: &str,
        wallet_password: &str,
        derive_password: Option<String>,
        derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
    ) -> Result<(), crate::ServiceError> {
        let start = std::time::Instant::now();
        let mut tx = self.repo;
        let dirs = crate::manager::Context::get_global_dirs()?;
        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let wallet = tx
            .wallet_detail_by_address(wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        // 获取种子
        let seed = WalletDomain::get_seed(dirs, &wallet.address, wallet_password).await?;
        // 获取默认链和币
        let default_chain_list = tx.get_chain_list().await?;
        let default_coins_list = tx.default_coin_list().await?;

        // 根据派生路径
        let hd_path = if let Some(derivation_path) = &derivation_path {
            let hd_path = wallet_chain_instance::derivation_path::get_account_hd_path_from_path(
                derivation_path,
            )?;
            Some(hd_path)
        } else {
            None
        };

        // 如果有指定派生路径，就获取该链的所有chain_code
        let chains: Vec<String> = if let Some(hd_path) = &hd_path {
            hd_path
                .get_chain_codes()?
                .0
                .into_iter()
                .map(|path| path.to_string())
                .collect()
        }
        // 或者使用默认链的链码
        else {
            default_chain_list
                .iter()
                .map(|chain| chain.chain_code.clone())
                .collect()
        };

        // 获取该账户的最大索引，并加一
        let account_index_map = if let Some(index) = index {
            let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
            tracing::warn!("account index: {:#?}", index);
            if tx.has_account_id(&wallet.address, index.account_id).await? {
                return Err(crate::ServiceError::Business(
                    crate::BusinessError::Account(crate::AccountError::AlreadyExist),
                ));
            };
            index
        } else if let Some(hd_path) = hd_path {
            wallet_utils::address::AccountIndexMap::from_index(hd_path.get_account_id()?)?
        } else if let Some(max_account) = tx
            .account_detail_by_max_id_and_wallet_address(&wallet.address)
            .await?
        {
            wallet_utils::address::AccountIndexMap::from_account_id(max_account.account_id + 1)?
        } else {
            wallet_utils::address::AccountIndexMap::from_account_id(1)?
        };

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut subkeys = Vec::<wallet_tree::io::BulkSubkey>::new();
        for chain_code in &chains {
            let btc_address_types = if chain_code == "btc" {
                BTC_ADDRESS_TYPES.to_vec()
            } else {
                vec![AddressType::Other]
            };
            let code: ChainCode = chain_code.as_str().try_into()?;
            for btc_address_type in btc_address_types {
                let Some(chain) = tx.detail_with_node(chain_code).await? else {
                    continue;
                };
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &btc_address_type, chain.network.as_str().into()).try_into()?;

                let (account_address, derivation_path) =
                    AccountDomain::create_account_with_derivation_path(
                        &mut tx,
                        &seed,
                        &instance,
                        derivation_path.as_deref(),
                        &account_index_map,
                        &wallet.uid,
                        &wallet.address,
                        name,
                        is_default_name,
                    )
                    .await?;

                let keypair = instance
                    .gen_keypair_with_index_address_type(&seed, account_index_map.input_index)
                    .map_err(|e| crate::SystemError::Service(e.to_string()))?;
                let pk = keypair.private_key_bytes()?;
                let subkey = wallet_tree::io::BulkSubkey::new(
                    account_index_map.clone(),
                    &account_address.address,
                    chain_code,
                    derivation_path.as_str(),
                    pk,
                );
                subkeys.push(subkey);
                for coin in &default_coins_list {
                    if &coin.chain_code == chain_code {
                        let assets_id =
                            AssetsId::new(&account_address.address, chain_code, &coin.symbol);
                        let assets = CreateAssetsVo::new(
                            assets_id,
                            coin.decimals,
                            coin.token_address.clone(),
                            coin.protocol.clone(),
                            0,
                        )
                        .with_name(&coin.name)
                        .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                        if coin.price.is_empty() {
                            req.insert(
                                chain_code,
                                &assets.token_address.clone().unwrap_or_default(),
                            );
                        }
                        tx.upsert_assets(assets).await?;
                    }
                }
            }
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

        // let accounts = tx.list().await?;

        // let mut device_bind_address_req =
        //     wallet_transport_backend::request::DeviceBindAddressReq::new(&device.sn);
        // for account in accounts {
        //     device_bind_address_req.push(&account.chain_code, &account.address);
        // }

        let device_bind_address_task_data =
            domain::app::DeviceDomain::gen_device_bind_address_task_data(&device.sn).await?;

        // let device_bind_address_task_data = crate::domain::task_queue::BackendApiTaskData::new(
        //     wallet_transport_backend::consts::endpoint::DEVICE_BIND_ADDRESS,
        //     &device_bind_address_req,
        // )?;
        let uid = wallet.uid;
        let task = Task::Common(CommonTask::QueryCoinPrice(req));
        Tasks::new()
            .push(task)
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                device_bind_address_task_data,
            )))
            .push(Task::Common(CommonTask::RecoverMultisigAccountData(uid)))
            .send()
            .await?;
        tracing::info!("cose time: {}", start.elapsed().as_millis());
        Ok(())
    }

    pub async fn list_derived_addresses(
        self,
        wallet_address: &str,
        index: i32,
        password: &str,
        all: bool,
    ) -> Result<Vec<DerivedAddressesList>, crate::ServiceError> {
        let mut tx = self.repo;

        WalletDomain::validate_password(password).await?;

        let account_index_map = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
        let dirs = crate::manager::Context::get_global_dirs()?;

        let root_dir = dirs.get_root_dir(wallet_address)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let seed = wallet_tree::api::KeystoreApi::load_seed(
            &wallet_tree,
            &root_dir,
            wallet_address,
            password,
        )?;

        // 获取默认链和币
        let chains = if !all {
            vec![
                "btc".to_string(),
                "eth".to_string(),
                "tron".to_string(),
                "sol".to_string(),
            ]
        } else {
            let default_chain_list = tx.get_chain_list().await?;
            // 如果有指定派生路径，就获取该链的所有chain_code
            default_chain_list
                .iter()
                .map(|chain| chain.chain_code.clone())
                .collect()
        };

        let mut res = Vec::new();
        for chain_code in &chains {
            let btc_address_types = if chain_code == "btc" {
                BTC_ADDRESS_TYPES.to_vec()
            } else {
                vec![AddressType::Other]
            };
            let code: ChainCode = chain_code.as_str().try_into()?;
            for btc_address_type in btc_address_types {
                let Some(chain) = tx.detail_with_node(chain_code).await? else {
                    continue;
                };
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &btc_address_type, chain.network.as_str().into()).try_into()?;

                let keypair = instance
                    .gen_keypair_with_index_address_type(&seed, account_index_map.input_index)?;

                let address_type = instance.address_type().into();
                let derivation_path = keypair.derivation_path();
                let address = keypair.address();
                res.push(DerivedAddressesList::new(
                    &address,
                    &derivation_path,
                    chain_code,
                    address_type,
                ));
            }
        }

        Ok(res)
    }

    pub async fn account_details(
        self,
        address: &str,
    ) -> Result<Option<AccountEntity>, crate::ServiceError> {
        let mut tx = self.repo;
        let res = AccountRepoTrait::detail(&mut tx, address).await?;
        Ok(res)
    }

    pub async fn edit_account_name(
        self,
        account_id: u32,
        wallet_address: &str,
        name: &str,
    ) -> Result<(), crate::ServiceError> {
        // let mut tx = self.repo.begin_transaction().await?;
        let mut tx = self.repo;
        let res = tx
            .edit_account_name(account_id, wallet_address, name)
            .await?;

        // tx.commit_transaction().await?;
        let Some(wallet) = tx.wallet_detail_by_address(wallet_address).await? else {
            return Err(crate::BusinessError::Wallet(crate::WalletError::NotFound).into());
        };
        let account_index_map =
            wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;
        for account in res {
            AccountDomain::address_init(
                &mut tx,
                &wallet.uid,
                &account.address,
                account_index_map.input_index,
                &account.chain_code,
                name,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn physical_delete_account(
        self,
        wallet_address: &str,
        account_id: u32,
        password: &str,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        WalletDomain::validate_password(password).await?;
        // Check if this is the last account
        let account_count = tx.count_unique_account_ids(wallet_address).await?;
        if account_count <= 1 {
            return Err(crate::BusinessError::Account(
                crate::AccountError::CannotDeleteLastAccount,
            )
            .into());
        }

        let deleted =
            AccountRepoTrait::physical_delete(&mut tx, wallet_address, account_id).await?;

        // domain::multisig::MultisigDomain::unbind_deleted_account_multisig_relations(
        //     &deleted, &device.sn,
        // )
        // .await?;

        let device_unbind_address_task =
            domain::app::DeviceDomain::gen_device_unbind_all_address_task_data(
                &deleted,
                Vec::new(),
                &device.sn,
            )
            .await?;

        let device_unbind_address_task =
            Task::BackendApi(BackendApiTask::BackendApi(device_unbind_address_task));
        Tasks::new().push(device_unbind_address_task).send().await?;
        let dirs = crate::manager::Context::get_global_dirs()?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        // let subs_path = dirs.get_subs_dir(wallet_address)?;

        wallet_tree.io().delete_account(
            &AccountIndexMap::from_account_id(account_id)?,
            &dirs.get_subs_dir(wallet_address)?,
        )?;

        // for del in deleted {
        //     wallet_tree.delete_subkey(
        //         // naming,
        //         wallet_address,
        //         &del.address,
        //         &del.chain_code,
        //         &subs_path,
        //         password,
        //     )?;
        //     // wallet_tree.delete_subkey(
        //     //     wallet_address,
        //     //     &subs_path,
        //     //     &del.address,
        //     //     &del.chain_code.as_str().try_into()?,
        //     // )?;
        //     req.push(&del.chain_code, &del.address);
        // }

        Ok(())
    }

    pub async fn set_all_password(
        &mut self,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        // let dirs = crate::manager::Context::get_global_dirs()?;
        WalletDomain::validate_password(old_password).await?;
        let tx = &mut self.repo;
        // let account_list = tx.list().await?;

        let indices = tx.get_all_account_indices().await?;

        // let Some(device) = tx.get_device_info().await? else {
        //     return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        // };
        // let old_encrypted_password = WalletDomain::encrypt_password(old_password, &device.sn)?;

        // if let Some(password) = &device.password {
        //     if password != &old_encrypted_password {
        //         return Err(
        //             crate::BusinessError::Wallet(crate::WalletError::PasswordIncorrect).into(),
        //         );
        //     }
        // }
        // let new_encrypted_password = WalletDomain::encrypt_password(new_password, &device.sn)?;
        // tx.update_password(Some(&new_encrypted_password)).await?;

        let wallet_list = tx.wallet_list().await?;

        tracing::info!("wallet_list: {:?}", wallet_list);
        // tracing::info!("account_list: {:?}", account_list);
        for wallet in wallet_list {
            self.set_root_password(&wallet.address, old_password, new_password)
                .await?;

            for index in &indices {
                let account_index_map = AccountIndexMap::from_account_id(*index)?;
                self.set_account_password(
                    &wallet.address,
                    &account_index_map,
                    old_password,
                    new_password,
                )
                .await?;
                // self.set_sub_password(
                //     &account.address,
                //     &account.chain_code,
                //     old_password,
                //     new_password,
                // )
                // .await?;
            }
        }

        self.set_verify_password(new_password).await?;
        Ok(())
    }

    pub async fn set_verify_password(&mut self, password: &str) -> Result<(), crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        wallet_tree::api::KeystoreApi::remove_verify_file(&dirs.root_dir)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        wallet_tree::api::KeystoreApi::store_verify_file(&wallet_tree, &dirs.root_dir, password)?;

        Ok(())
    }

    pub async fn set_root_password(
        &mut self,
        wallet_address: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        // let tx = &mut self.repo;

        let dirs = crate::manager::Context::get_global_dirs()?;
        let db = crate::manager::Context::get_global_sqlite_pool()?;

        let wallet = WalletEntity::detail(db.as_ref(), wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        // Get the path to the root directory for the given wallet name.
        let root_dir = dirs.get_root_dir(&wallet.address)?;

        // Traverse the directory structure to obtain the current wallet tree.

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        Ok(wallet_tree::api::KeystoreApi::update_root_password(
            root_dir,
            wallet_tree,
            wallet_address,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?)
    }

    pub async fn set_sub_password(
        &mut self,
        // wallet_address: &str,
        address: &str,
        chain_code: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        let tx = &mut self.repo;

        let dirs = crate::manager::Context::get_global_dirs()?;
        let db = crate::manager::Context::get_global_sqlite_pool()?;
        let req = wallet_database::entities::account::QueryReq {
            wallet_address: None,
            address: Some(address.to_string()),
            chain_code: Some(chain_code.to_string()),
            account_id: None,
            status: Some(1),
        };
        let account = AccountEntity::detail(db.as_ref(), &req)
            .await?
            .ok_or(crate::BusinessError::Account(crate::AccountError::NotFound))?;

        // Get the path to the subkeys directory for the given wallet name.
        let subs_dir = dirs.get_subs_dir(&account.wallet_address)?;

        // Traverse the directory structure to obtain the current wallet tree.
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let Some(chain) = tx.detail_with_node(chain_code).await? else {
            return Err(crate::ServiceError::Business(crate::BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_string()),
            )));
        };
        let instance = wallet_chain_instance::instance::ChainObject::new(
            chain_code,
            account.address_type(),
            chain.network.as_str().into(),
        )?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        Ok(wallet_tree::api::KeystoreApi::update_child_password(
            subs_dir,
            instance,
            wallet_tree,
            &account.wallet_address,
            address,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?)
    }

    pub async fn set_account_password(
        &self,
        wallet_address: &str,
        account_index_map: &wallet_utils::address::AccountIndexMap,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        let subs_dir = dirs.get_subs_dir(wallet_address)?;

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
        wallet_tree::api::KeystoreApi::update_account_password(
            wallet_tree,
            &subs_dir,
            account_index_map,
            old_password,
            new_password,
            algorithm,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?;

        Ok(())
    }

    pub async fn get_account_private_key(
        &mut self,
        password: &str,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<crate::response_vo::account::GetAccountPrivateKeyRes, crate::ServiceError> {
        let tx = &mut self.repo;

        // let Some(device) = tx.get_device_info().await? else {
        //     return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        // };
        // WalletDomain::validate_password(password).await?;

        let account_list = tx
            .account_list_by_wallet_address_and_account_id_and_chain_codes(
                Some(wallet_address),
                Some(account_id),
                Vec::new(),
            )
            .await?;
        let chains = tx.get_chain_list().await?;

        let mut res = Vec::new();
        tracing::info!("account_list: {}", account_list.len());
        for account in account_list {
            let private_key = crate::domain::account::open_account_pk_with_password(
                &account.chain_code,
                &account.address,
                password,
            )
            .await?;

            let btc_address_type_opt: AddressType = account.address_type().try_into()?;
            if let Some(chain) = chains
                .iter()
                .find(|chain| chain.chain_code == account.chain_code)
            {
                let data = crate::response_vo::account::GetAccountPrivateKey {
                    chain_code: account.chain_code,
                    name: chain.name.clone(),
                    address: account.address,
                    address_type: btc_address_type_opt.into(),
                    private_key: private_key.to_string(),
                };
                res.push(data);
            }
        }

        Ok(crate::response_vo::account::GetAccountPrivateKeyRes(res))
    }

    pub async fn get_account_list(
        &mut self,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
    ) -> Result<Vec<AccountEntity>, crate::ServiceError> {
        Ok(self
            .repo
            .get_account_list_by_wallet_address_and_account_id(wallet_address, account_id)
            .await?)
    }

    pub async fn get_account_address(
        &mut self,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<crate::response_vo::account::GetAccountAddressRes, crate::ServiceError> {
        let tx = &mut self.repo;
        let account_list = tx
            .get_account_list_by_wallet_address_and_account_id(
                Some(wallet_address),
                Some(account_id),
            )
            .await?;

        let mut res = Vec::new();
        for account in account_list {
            let address_type = account.address_type().try_into()?;
            let data = crate::response_vo::account::GetAccountAddress {
                chain_code: account.chain_code,
                address: account.address,
                address_type,
            };
            res.push(data);
        }

        Ok(crate::response_vo::account::GetAccountAddressRes(res))
    }

    // pub fn recover_subkey(
    //     &self,
    //     wallet_name: &str,
    //     address: &str,
    // ) -> Result<(), crate::ServiceError> {
    //     let dirs = crate::manager::Context::get_global_dirs()?;
    //     // Get the path to the subkeys directory for the given wallet name.
    //     let subs_path = dirs.get_subs_dir(wallet_name)?;

    //     // Traverse the directory structure to obtain the wallet tree.
    //     let mut wallet_tree =
    //         wallet_tree::wallet_tree::WalletTree::traverse_directory_structure(&dirs.wallet_dir)?;

    //     // Call the recover_subkey function to recover the subkey,
    //     // passing in the wallet tree, address, subkeys path, and wallet name.
    //     let wallet = wallet_tree.get_mut_wallet_branch(wallet_name)?;
    //     Ok(wallet.recover_subkey(address, subs_path)?)
    // }
}
