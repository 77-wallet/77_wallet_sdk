use wallet_database::{
    entities::account::AccountEntity,
    repositories::{
        ResourcesRepo,
        account::{AccountRepo, AccountRepoTrait},
        chain::ChainRepo,
        coin::CoinRepo,
        device::DeviceRepo,
        multisig_account::MultisigAccountRepo,
        wallet::WalletRepoTrait,
    },
};
use wallet_transport_backend::request::{
    AddressBatchInitReq, AddressUpdateAccountNameReq, TokenQueryPriceReq,
};
use wallet_tree::api::KeystoreApi;
use wallet_types::{chain::chain::ChainCode, constant::chain_code};
use wallet_utils::address::AccountIndexMap;

use crate::{
    domain::{
        self, account::AccountDomain, app::config::ConfigDomain, chain::ChainDomain,
        permission::PermissionDomain, wallet::WalletDomain,
    },
    infrastructure::task_queue::{
        BackendApiTask, BackendApiTaskData, CommonTask, RecoverDataBody, task::Tasks,
    },
    response_vo::account::{CurrentAccountInfo, DerivedAddressesList, QueryAccountDerivationPath},
};

pub struct AccountService {
    pub repo: ResourcesRepo,
    pub wallet_domain: WalletDomain,
    // keystore: wallet_crypto::Keystore
}

impl AccountService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo, wallet_domain: WalletDomain::new() }
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
        derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut tx = self.repo;
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs()?;

        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let wallet = tx
            .wallet_detail_by_address(wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        // 获取种子
        let seed = WalletDomain::get_seed(dirs.as_ref(), &wallet.address, wallet_password).await?;
        // 获取默认链和币
        let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
        let default_coins_list = CoinRepo::default_coin_list(&pool).await?;

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
            hd_path.get_chain_codes()?.0.into_iter().map(|path| path.to_string()).collect()
        }
        // 或者使用默认链的链码
        else {
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        // 获取该账户的最大索引，并加一
        let account_index_map = if let Some(index) = index {
            let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
            if tx.has_account_id(&wallet.address, index.account_id).await? {
                return Err(crate::ServiceError::Business(crate::BusinessError::Account(
                    crate::AccountError::AlreadyExist,
                )));
            };
            index
        } else if let Some(hd_path) = hd_path {
            wallet_utils::address::AccountIndexMap::from_index(hd_path.get_account_id()?)?
        } else if let Some(max_account) =
            tx.account_detail_by_max_id_and_wallet_address(&wallet.address).await?
        {
            wallet_utils::address::AccountIndexMap::from_account_id(max_account.account_id + 1)?
        } else {
            wallet_utils::address::AccountIndexMap::from_account_id(1)?
        };

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        let mut subkeys = Vec::<wallet_tree::file_ops::BulkSubkey>::new();

        let mut address_batch_init_task_data = AddressBatchInitReq(Vec::new());

        ChainDomain::init_chains_assets(
            &mut tx,
            &default_coins_list,
            &mut req,
            &mut address_batch_init_task_data,
            &mut subkeys,
            &chains,
            &seed,
            &account_index_map,
            derivation_path.as_deref(),
            &wallet.uid,
            &wallet.address,
            name,
            is_default_name,
        )
        .await?;

        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;
        let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;

        let tron_address =
            subkeys.iter().find(|s| s.chain_code == chain_code::TRON).map(|s| s.address.clone());

        KeystoreApi::initialize_child_keystores(
            wallet_tree,
            subkeys,
            dirs.get_subs_dir(wallet_address)?,
            wallet_password,
            algorithm,
        )?;

        // let device_bind_address_task_data =
        //     domain::app::DeviceDomain::gen_device_bind_address_task_data().await?;

        // 恢复多签账号、多签队列
        let mut body = RecoverDataBody::new(&wallet.uid);
        if let Some(tron_address) = tron_address {
            body.tron_address = Some(tron_address);
        };

        let address_batch_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_BATCH_INIT,
            &address_batch_init_task_data,
        )?;
        Tasks::new()
            .push(CommonTask::QueryCoinPrice(req))
            // .push(Task::BackendApi(BackendApiTask::BackendApi(
            //     device_bind_address_task_data,
            // )))
            .push(CommonTask::RecoverMultisigAccountData(body))
            .push(BackendApiTask::BackendApi(address_batch_init_task_data))
            .send()
            .await?;
        // for task in address_init_task_data {
        //     Tasks::new()
        //         .push(Task::BackendApi(BackendApiTask::BackendApi(task)))
        //         .send()
        //         .await?;
        // }

        Ok(())
    }

    pub async fn get_account_derivation_path(
        self,
        wallet_address: &str,
        index: u32,
    ) -> Result<Vec<QueryAccountDerivationPath>, crate::ServiceError> {
        let mut tx = self.repo;
        let list = tx
            .get_account_list_by_wallet_address_and_account_id(Some(wallet_address), Some(index))
            .await?;
        let mut res = Vec::new();
        for data in list {
            let address_type =
                AccountDomain::get_show_address_type(&data.chain_code, data.address_type())?;
            res.push(QueryAccountDerivationPath::new(
                &data.address,
                &data.derivation_path,
                &data.chain_code,
                address_type,
            ));
        }

        Ok(res)
    }

    pub async fn list_derived_addresses(
        self,
        wallet_address: &str,
        index: i32,
        password: &str,
        all: bool,
    ) -> Result<Vec<DerivedAddressesList>, crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut tx = self.repo;

        WalletDomain::validate_password(password).await?;

        let account_index_map = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs()?;

        let root_dir = dirs.get_root_dir(wallet_address)?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let seed = wallet_tree::api::KeystoreApi::load_seed(
            &*wallet_tree,
            &root_dir,
            wallet_address,
            password,
        )?;

        // 获取默认链和币
        let chains = if !all {
            vec!["btc".to_string(), "eth".to_string(), "tron".to_string(), "sol".to_string()]
        } else {
            let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
            // 如果有指定派生路径，就获取该链的所有chain_code
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        let mut res = Vec::new();
        for chain in chains.iter() {
            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = ChainDomain::get_node(chain).await else {
                continue;
            };
            for address_type in address_types {
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &address_type, node.network.as_str().into()).try_into()?;

                let keypair = instance
                    .gen_keypair_with_index_address_type(&seed, account_index_map.input_index)?;

                let address_type = instance.address_type().into();
                let derivation_path = keypair.derivation_path();
                let address = keypair.address();

                let mut derived_address = DerivedAddressesList::new(
                    &address,
                    &derivation_path,
                    &node.chain_code,
                    address_type,
                );

                match code {
                    ChainCode::Solana | ChainCode::Sui | ChainCode::Ton => {
                        let account =
                            tx.detail_by_address_and_chain_code(&address, &node.chain_code).await?;
                        if let Some(account) = account {
                            derived_address.with_mapping_account(account.account_id, account.name);
                        };

                        if account_index_map.input_index < 0 {
                            derived_address
                                .with_mapping_positive_index(account_index_map.unhardend_index);
                        }
                    }
                    _ => {}
                }

                res.push(derived_address);
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
        tx.edit_account_name(account_id, wallet_address, name).await?;

        // tx.commit_transaction().await?;
        let Some(wallet) = tx.wallet_detail_by_address(wallet_address).await? else {
            return Err(crate::BusinessError::Wallet(crate::WalletError::NotFound).into());
        };
        let account_index_map =
            wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;

        let req =
            AddressUpdateAccountNameReq::new(&wallet.uid, account_index_map.input_index, name);
        let req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_UPDATE_ACCOUNT_NAME,
            &req,
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(req)).send().await?;

        Ok(())
    }

    pub async fn physical_delete_account(
        self,
        wallet_address: &str,
        account_id: u32,
        password: &str,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(&pool).await? else {
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

        let device_unbind_address_task =
            domain::app::DeviceDomain::gen_device_unbind_all_address_task_data(
                &deleted,
                Vec::new(),
                &device.sn,
            )
            .await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // delete permission
        for account in deleted.iter() {
            if account.chain_code == chain_code::TRON {
                PermissionDomain::delete_by_address(&pool, &account.address).await?;
            }
        }

        Tasks::new().push(BackendApiTask::BackendApi(device_unbind_address_task)).send().await?;
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs()?;
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        wallet_tree.io().delete_account(
            &AccountIndexMap::from_account_id(account_id)?,
            &dirs.get_subs_dir(wallet_address)?,
        )?;

        Ok(())
    }

    pub async fn set_all_password(
        &mut self,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        WalletDomain::validate_password(old_password).await?;
        let tx = &mut self.repo;

        let indices = tx.get_all_account_indices().await?;
        let wallet_list = tx.wallet_list().await?;

        for wallet in wallet_list {
            AccountDomain::set_root_password(&wallet.address, old_password, new_password).await?;
            for index in &indices {
                let account_index_map = AccountIndexMap::from_account_id(*index)?;
                AccountDomain::set_account_password(
                    &wallet.address,
                    &account_index_map,
                    old_password,
                    new_password,
                )
                .await?;
            }
        }

        AccountDomain::set_verify_password(new_password).await?;
        Ok(())
    }

    pub async fn set_sub_password(
        &mut self,
        address: &str,
        chain_code: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs()?;
        let db = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let req = wallet_database::entities::account::QueryReq {
            wallet_address: None,
            address: Some(address.to_string()),
            chain_code: Some(chain_code.to_string()),
            account_id: None,   
            status: Some(1),
        };
        let account = AccountEntity::detail(db.as_ref(), &req).await?.ok_or(
            crate::BusinessError::Account(crate::AccountError::NotFound(address.to_string())),
        )?;

        // Get the path to the subkeys directory for the given wallet name.
        let subs_dir = dirs.get_subs_dir(&account.wallet_address)?;

        // Traverse the directory structure to obtain the current wallet tree.
        let wallet_tree_strategy = ConfigDomain::get_wallet_tree_strategy().await?;
        let wallet_tree = wallet_tree_strategy.get_wallet_tree(&dirs.wallet_dir)?;

        let node = ChainDomain::get_node(chain_code).await?;

        let instance = wallet_chain_instance::instance::ChainObject::new(
            chain_code,
            account.address_type(),
            node.network.as_str().into(),
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

    pub async fn get_account_private_key(
        &mut self,
        password: &str,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<crate::response_vo::account::GetAccountPrivateKeyRes, crate::ServiceError> {
        WalletDomain::validate_password(password).await?;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let tx = &mut self.repo;

        let account_list = tx
            .account_list_by_wallet_address_and_account_id_and_chain_codes(
                Some(wallet_address),
                Some(account_id),
                Vec::new(),
            )
            .await?;
        let chains = ChainRepo::get_chain_list(&pool).await?;

        let mut res = Vec::new();

        let account_index_map = AccountIndexMap::from_account_id(account_id)?;

        let data = domain::account::open_accounts_pk_with_password(
            &account_index_map,
            wallet_address,
            password,
        )
        .await?;
        for account in account_list {
            if let Some((_, pk)) = data.iter().find(|(meta, _)| {
                meta.chain_code == account.chain_code
                    && meta.address == account.address
                    && meta.derivation_path == account.derivation_path
            }) {
                let address_type = AccountDomain::get_show_address_type(
                    &account.chain_code,
                    account.address_type(),
                )?;
                if let Some(chain) =
                    chains.iter().find(|chain| chain.chain_code == account.chain_code)
                {
                    let data = crate::response_vo::account::GetAccountPrivateKey {
                        chain_code: account.chain_code,
                        name: chain.name.clone(),
                        address: account.address,
                        address_type,
                        private_key: pk.to_string(),
                    };
                    res.push(data);
                }
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

    pub async fn current_chain_address(
        uid: String,
        account_id: u32,
        chain_code: &str,
    ) -> Result<Vec<QueryAccountDerivationPath>, crate::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let res = AccountRepo::current_chain_address(uid, account_id, chain_code, &pool).await?;

        let result = res
            .into_iter()
            .map(QueryAccountDerivationPath::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(result)
    }

    pub async fn current_accounts(
        &mut self,
        wallet_address: &str,
        account_id: i32,
    ) -> Result<Vec<CurrentAccountInfo>, crate::ServiceError> {
        let accounts = self
            .repo
            .get_account_list_by_wallet_address_and_account_id(
                Some(wallet_address),
                Some(account_id as u32),
            )
            .await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = MultisigAccountRepo::new(pool);

        let mut result = vec![];
        for account in accounts.into_iter() {
            let is_multisig = if account.chain_code == chain_code::TRON {
                repo.found_by_address(&account.address).await?.is_some()
            } else {
                false
            };
            let address_type =
                AccountDomain::get_show_address_type(&account.chain_code, account.address_type())?;

            result.push(CurrentAccountInfo {
                chain_code: account.chain_code,
                address: account.address,
                address_type,
                is_multisig,
            });
        }

        Ok(result)
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
