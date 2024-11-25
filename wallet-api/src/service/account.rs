use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{account::AccountEntity, assets::AssetsId, wallet::WalletEntity},
    repositories::{
        account::AccountRepoTrait, assets::AssetsRepoTrait, chain::ChainRepoTrait,
        coin::CoinRepoTrait, wallet::WalletRepoTrait, ResourcesRepo, TransactionTrait as _,
    },
};
use wallet_transport_backend::request::TokenQueryPriceReq;
use wallet_types::chain::{
    address::r#type::{AddressType, BTC_ADDRESS_TYPES},
    chain::ChainCode,
};

use crate::{
    domain::{self, account::AccountDomain, task_queue::Tasks, wallet::WalletDomain},
    response_vo::account::DerivedAddressesList,
};

pub struct AccountService {
    pub repo: ResourcesRepo,
    pub account_domain: AccountDomain,
    pub wallet_domain: WalletDomain,
    // keystore: wallet_keystore::Keystore
}

impl AccountService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            account_domain: AccountDomain::new(),
            wallet_domain: WalletDomain::new(),
        }
    }

    pub async fn create_account(
        self,
        wallet_address: &str,
        wallet_password: &str,
        derive_password: Option<String>,
        derivation_path: Option<String>,
        index: Option<i32>,
        name: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let dirs = crate::manager::Context::get_global_dirs()?;

        // 根据钱包地址查询是否有钱包
        let wallet = tx
            .wallet_detail_by_address(wallet_address)
            .await?
            .ok_or(crate::BusinessError::Wallet(crate::WalletError::NotFound))?;

        // 获取种子
        let seed_wallet =
            self.wallet_domain
                .get_seed_wallet(dirs, &wallet.address, wallet_password)?;
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
        tracing::info!("account_index_map: {:?}", account_index_map);

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
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

                let res = self
                    .account_domain
                    .create_account_with_derivation_path(
                        &mut tx,
                        dirs,
                        &seed_wallet.seed,
                        instance,
                        &derivation_path,
                        &account_index_map,
                        &wallet.uid,
                        &wallet.address,
                        wallet_password,
                        derive_password.clone(),
                        &name,
                    )
                    .await?;
                for coin in &default_coins_list {
                    if &coin.chain_code == chain_code {
                        let assets_id = AssetsId::new(&res.address, chain_code, &coin.symbol);
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
        let task =
            domain::task_queue::Task::Common(domain::task_queue::CommonTask::QueryCoinPrice(req));
        Tasks::new().push(task).send().await?;
        Ok(())
    }

    pub async fn list_derived_addresses(
        self,
        wallet_address: &str,
        index: i32,
        password: &str,
        all: bool,
    ) -> Result<Vec<DerivedAddressesList>, crate::ServiceError> {
        let account_index_map = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
        let dirs = crate::manager::Context::get_global_dirs()?;

        let root_dir = dirs.get_root_dir(wallet_address)?;
        let seed =
            wallet_keystore::api::KeystoreApi::load_seed(root_dir, wallet_address, password)?;

        let mut tx = self.repo;
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
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;

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
        let dirs = crate::manager::Context::get_global_dirs()?;
        let mut wallet_tree =
            wallet_tree::wallet_tree::WalletTree::traverse_directory_structure(&dirs.wallet_dir)?;
        let subs_path = dirs.get_subs_dir(wallet_address)?;
        for del in deleted {
            wallet_tree.delete_subkeys(
                wallet_address,
                &subs_path,
                &del.address,
                &del.chain_code.as_str().try_into()?,
            )?;
        }

        tx.commit_transaction().await?;
        Ok(())
    }

    pub async fn set_all_password(
        &mut self,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), crate::ServiceError> {
        // let dirs = crate::manager::Context::get_global_dirs()?;
        let tx = &mut self.repo;
        let account_list = tx.list().await?;

        let wallet_list = tx.wallet_list().await?;

        for wallet in wallet_list {
            self.set_root_password(&wallet.address, old_password, new_password)
                .await?;
        }

        for account in account_list {
            self.set_sub_password(
                &account.address,
                &account.chain_code,
                old_password,
                new_password,
            )
            .await?;
        }
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
        let wallet_tree =
            wallet_tree::wallet_tree::WalletTree::traverse_directory_structure(&dirs.wallet_dir)?;

        Ok(wallet_keystore::api::KeystoreApi::update_root_password(
            root_dir,
            wallet_tree,
            wallet_address,
            old_password,
            new_password,
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
        let wallet_tree =
            wallet_tree::wallet_tree::WalletTree::traverse_directory_structure(&dirs.wallet_dir)?;
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

        Ok(wallet_keystore::api::KeystoreApi::update_child_password(
            subs_dir,
            instance,
            wallet_tree,
            &account.wallet_address,
            address,
            old_password,
            new_password,
        )
        .map_err(|e| crate::SystemError::Service(e.to_string()))?)
    }

    pub async fn get_account_private_key(
        &mut self,
        password: &str,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<crate::response_vo::account::GetAccountPrivateKeyRes, crate::ServiceError> {
        let tx = &mut self.repo;

        let account_list = tx
            .account_list_by_wallet_address_and_account_id_and_chain_codes(
                Some(wallet_address),
                Some(account_id),
                Vec::new(),
            )
            .await?;

        let chains = tx.get_chain_list().await?;

        let mut res = Vec::new();
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

    pub fn recover_subkey(
        &self,
        wallet_name: &str,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        let dirs = crate::manager::Context::get_global_dirs()?;
        // Get the path to the subkeys directory for the given wallet name.
        let subs_path = dirs.get_subs_dir(wallet_name)?;

        // Traverse the directory structure to obtain the wallet tree.
        let mut wallet_tree =
            wallet_tree::wallet_tree::WalletTree::traverse_directory_structure(&dirs.wallet_dir)?;

        // Call the recover_subkey function to recover the subkey,
        // passing in the wallet tree, address, subkeys path, and wallet name.
        let wallet = wallet_tree.get_mut_wallet_branch(wallet_name)?;
        Ok(wallet.recover_subkey(address, subs_path)?)
    }
}
