use std::collections::HashSet;

use wallet_database::{
    entities::{
        api_assets::ApiAssetsEntity, api_coin::ApiCoinEntity, api_wallet::ApiWalletType,
        assets::AssetsIdVo, node::NodeEntity,
    },
    repositories::{
        ResourcesRepo, TransactionTrait as _,
        api_wallet::{
            account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo, coin::ApiCoinRepo,
            wallet::ApiWalletRepo,
        },
        node::{NodeRepo, NodeRepoTrait},
    },
};
use wallet_transport_backend::request::{
    ChainRpcListReq, TokenQueryPriceReq, api_wallet::address::ApiAddressInitReq,
};
use wallet_types::chain::chain::ChainCode;

use crate::{
    api::ReturnType,
    domain::{
        api_wallet::{account::ApiAccountDomain, assets::ApiAssetsDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        chain::{ChainDomain, NodeInfo},
        wallet::WalletDomain,
    },
    infrastructure::{
        asset_calc::actor_model::AssetKey,
        task_queue::{
            CommonTask,
            backend::{BackendApiTask, BackendApiTaskData},
            task::Tasks,
        },
    },
};

pub struct ApiChainDomain {}

impl ApiChainDomain {
    pub(crate) async fn init_chains_api_assets(
        coins: &[ApiCoinEntity],
        req: &mut TokenQueryPriceReq,
        api_address_init_req: &mut ApiAddressInitReq,
        chain_list: &[String],
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        account_name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<AssetKey>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut all_asset_keys = Vec::new();

        for chain in chain_list.iter() {
            // let index = account_index_map.input_index;
            // let mut params = AddressParam::new(index);
            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = ChainDomain::get_node(chain).await else {
                continue;
            };

            if let Some(account) = ApiAccountRepo::find_one_by_wallet_address_account_id_chain_code(
                &pool,
                &wallet_address,
                account_index_map.account_id,
                chain,
            )
            .await?
            {
                if account.is_init == 1 {
                    continue;
                }
            }

            for address_type in address_types {
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &address_type, node.network.as_str().into()).try_into()?;
                // (&code, &address_type, "mainnet".into()).try_into()?;
                let (account_address, address_init_req) = ApiAccountDomain::derive_subkey(
                    uid,
                    seed,
                    wallet_address,
                    account_index_map,
                    &instance,
                    account_name,
                    is_default_name,
                    api_wallet_type,
                )
                .await?;

                if let Some(address_init_req) = address_init_req {
                    api_address_init_req.address_list.add_address(address_init_req);
                }

                // 收集init_default_api_assets返回的AssetKey
                let asset_keys = ApiAssetsDomain::init_default_api_assets(
                    wallet_address,
                    coins,
                    &account_address,
                    &code.to_string(),
                    req,
                )
                .await?;

                all_asset_keys.extend(asset_keys);
            }

            // if !params.address_list.is_empty() {
            //     expand_address_req.add_chain_code(chain, params);
            // }
        }

        Ok(all_asset_keys)
    }

    pub(crate) async fn upsert_multi_api_chain_than_toggle(
        chains: wallet_transport_backend::response_vo::api_wallet::chain::ApiChainListResp,
    ) -> Result<Vec<String>, crate::error::service::ServiceError> {
        // tracing::warn!("upsert_multi_chain_than_toggle, chains: {:#?}", chains);
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        // // 本地后端节点
        // let local_backend_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(0)).await?;

        // // 本地配置节点
        // let default_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(1)).await?;

        let mut input = Vec::new();
        let mut chain_codes = Vec::new();
        // let mut has_new_chain = false;
        let account_list = ApiAccountRepo::list(&pool).await?;

        let mut new_chains = Vec::new();
        let app_version = ConfigDomain::get_app_version().await?.app_version;
        for chain in chains.0 {
            let Some(master_token_code) = chain.master_token_code else {
                continue;
            };

            let status = match (
                ConfigDomain::compare_versions(&app_version, &chain.app_version_code),
                chain.enable,
            ) {
                (std::cmp::Ordering::Less, _) => 0,
                (_, true) => 1,
                (_, false) => 0,
            };

            if account_list
                .iter()
                .all(|acc_chain| acc_chain.chain_code != chain.chain_code && chain.enable)
            {
                // has_new_chain = true;
                new_chains.push(chain.chain_code.clone());
            }

            // if local_backend_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // } else if default_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // }

            input.push(
                wallet_database::entities::api_chain::ApiChainCreateVo::new(
                    &chain.name,
                    &chain.chain_code,
                    &[],
                    &master_token_code,
                )
                .with_status(status),
            );
            if status == 1 {
                chain_codes.push(chain.chain_code);
            }
        }

        ApiChainRepo::upsert_multi_chain(&pool, input).await?;
        Self::toggle_api_chains(&chain_codes).await?;

        if !chain_codes.is_empty() {
            let chain_rpc_list_req = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                &ChainRpcListReq::new(chain_codes),
            )?;
            Tasks::new().push(BackendApiTask::BackendApi(chain_rpc_list_req)).send().await?;
        }

        Ok(new_chains)
    }

    pub async fn toggle_api_chains(
        chain_codes: &[String],
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiChainRepo::toggle_chains_status(&pool, chain_codes).await?;
        Ok(())
    }

    pub async fn init_api_chain_info() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let list = crate::default_data::chain::get_default_chains_list()?;

        // tracing::warn!("list {:#?}", list);

        let mut chain_codes = Vec::new();
        for (chain_code, default_chain) in &list.chains {
            let status = if default_chain.active { 1 } else { 0 };
            // let node_id =
            //     NodeDomain::gen_node_id(&default_chain.node_name, &default_chain.chain_code);
            let req = wallet_database::entities::api_chain::ApiChainCreateVo::new(
                &default_chain.name,
                &default_chain.chain_code,
                &default_chain.protocols,
                &default_chain.main_symbol,
            )
            .with_status(status);

            if let Err(e) = ApiChainRepo::add(&pool, req).await {
                tracing::error!("Failed to create default chain: {:?}", e);
                continue;
            }
            if status == 1 {
                chain_codes.push(chain_code.to_string());
            }
        }
        let app_version = ConfigDomain::get_app_version().await?;

        ApiChainRepo::toggle_chains_status(&pool, &chain_codes).await?;
        let chain_list_req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::api_wallet::API_WALLET_CHAIN_LIST,
            &wallet_transport_backend::request::ChainListReq::new(app_version.app_version),
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(chain_list_req)).send().await?;
        Ok(())
    }

    pub(crate) async fn sync_nodes_and_link_to_api_chains(
        repo: &mut ResourcesRepo,
        chain_code: &[String],
        backend_nodes: &[NodeEntity],
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut env = "mainnet".to_owned();

        #[cfg(feature = "test")]
        {
            env = "testnet".to_owned();
        }
        tracing::error!("sync_nodes_and_link_to_api_chains  now env is {}", env);

        // 本地的backend_nodes 和 backend_nodes 比较，把backend_nodes中没有，local_backend_nodes有的节点，删除
        let local_backend_nodes = NodeRepoTrait::list_by_chain(repo, &chain_code, Some(0)).await?;
        let backend_node_rpcs: HashSet<String> = backend_nodes
            .iter()
            .filter(|o| env == o.network)
            .filter(|node| chain_code.contains(&node.chain_code))
            .map(|n| n.node_id.clone())
            .collect();

        for node in local_backend_nodes {
            if node.network != env {
                tracing::error!("sync_nodes_and_link_to_api_chains  network ,{:?}", node);
            }
            if !backend_node_rpcs.contains(&node.node_id) {
                if let Err(e) = NodeRepoTrait::delete(repo, &node.node_id).await {
                    tracing::error!("Failed to remove filtered node {}: {:?}", node.node_id, e);
                }
                // tracing::error!(
                //     "---> set_api_chain_node chain_code {},{:?}",
                //     node.chain_code,
                //     backend_nodes
                // );
                Self::set_api_chain_node(repo, backend_nodes, &node.chain_code).await?;
            }
        }
        Self::assign_missing_nodes_to_api_chains(backend_nodes).await?;
        Ok(())
    }

    pub async fn init_bind_api_chain_node() -> ReturnType<()> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let api_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
        let chain_codes = api_chain_list.iter().map(|one| one.chain_code.clone()).collect();
        let req = ChainRpcListReq::new(chain_codes);
        let node_lists = NodeRepo::list(&pool, None).await?;
        ApiChainDomain::sync_nodes_and_link_to_api_chains(&mut repo, &req.chain_code, &node_lists)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_api_chain_node(
        repo: &mut ResourcesRepo,
        backend_nodes: &[NodeEntity],
        // default_nodes: &[NodeData],
        chain_code: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        tracing::info!("set_api_chain_node: chain_code: {}", chain_code);
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let list = NodeRepo::list(&pool, Some(1)).await?;

        let mut backend_nodes_filter = Vec::new();
        for backend_node in backend_nodes.iter() {
            #[cfg(feature = "test")]
            if backend_node.network != "testnet" {
                continue;
            }
            #[cfg(feature = "prod")]
            if backend_node.network != "mainnet" {
                continue;
            }
            #[cfg(feature = "dev")]
            if backend_node.network != "testnet" {
                continue;
            }
            backend_nodes_filter.push(backend_node);
        }

        let mut default_nodes = Vec::new();
        for default_node in list.iter() {
            // let node_id = NodeDomain::gen_node_id(&default_node.name, &default_node.chain_code);
            #[cfg(feature = "test")]
            if default_node.network != "testnet" {
                continue;
            }
            #[cfg(feature = "prod")]
            if default_node.network != "mainnet" {
                continue;
            }
            #[cfg(feature = "dev")]
            if default_node.network != "testnet" {
                continue;
            }
            default_nodes.push(wallet_types::valueobject::NodeData::new(
                &default_node.node_id,
                &default_node.rpc_url,
                &default_node.chain_code,
            ));
        }

        repo.begin_transaction().await?;
        tracing::debug!("set_api_chain_node: backend_nodes: {:?}", backend_nodes_filter);
        if let Some(backend_nodes) =
            backend_nodes_filter.iter().find(|node| node.chain_code == chain_code)
        {
            tracing::debug!("设置后端节点: backend_nodes: {:?}", backend_nodes);
            if let Err(e) =
                ApiChainRepo::set_api_chain_node(&pool, chain_code, &backend_nodes.node_id).await
            {
                tracing::error!("set_api_chain_node error: {:?}", e);
            }
        } else if let Some(node) = default_nodes.iter().find(|node| node.chain_code == chain_code) {
            tracing::debug!("设置默认节点: node: {:?}", node);
            if let Err(e) = ApiChainRepo::set_api_chain_node(&pool, chain_code, &node.node_id).await
            {
                tracing::error!("set_api_chain_node error: {:?}", e);
            }
        }
        repo.commit_transaction().await?;
        tracing::debug!("set_api_chain_node done: chain_code: {}", chain_code);
        Ok(())
    }

    pub(crate) async fn assign_missing_nodes_to_api_chains(
        backend_nodes: &[NodeEntity],
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        for chain in chain_list {
            if chain.node_id.is_none() {
                tracing::debug!(
                    "[assign_missing_nodes_to_chains] set chain node: {}",
                    chain.chain_code
                );
                Self::set_api_chain_node(&mut repo, backend_nodes, &chain.chain_code).await?;
            }
        }
        tracing::debug!("[assign_missing_nodes_to_chains] end");
        Ok(())
    }

    pub async fn sync_withdrawal_wallet_chain_data()
    -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let password = ApiWalletDomain::get_passwd().await?;

        let chain_list: Vec<String> = ApiChainRepo::get_chain_list(&pool)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect();

        let withdrawal_wallet_list =
            ApiWalletRepo::list(pool.as_ref(), Some(ApiWalletType::Withdrawal)).await?;

        for wallet in withdrawal_wallet_list {
            ApiAccountDomain::create_withdrawal_account(
                &wallet.address,
                &password,
                chain_list.clone(),
                "账户",
                true,
            )
            .await?;
        }

        Ok(())
    }

    pub(crate) async fn get_node(
        chain_code: &str,
    ) -> Result<NodeInfo, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let node = match ApiChainRepo::detail_with_node(&pool, chain_code).await? {
            Some(node) => NodeInfo::new(
                &node.chain_code,
                &node.node_id,
                &node.node_name,
                &node.rpc_url,
                &node.ws_url,
                &node.http_url,
                &node.network,
                node.status,
            ),
            None => {
                let node = NodeRepo::get_local_node_by_chain(&pool, chain_code)
                    .await?
                    .pop()
                    .ok_or(crate::error::business::BusinessError::ChainNode(
                        crate::error::business::chain_node::ChainNodeError::NodeNotFound,
                    ))?;
                NodeInfo::new(
                    &node.chain_code,
                    &node.node_id,
                    &node.name,
                    &node.rpc_url,
                    &node.ws_url,
                    &node.http_url,
                    &node.network,
                    node.status,
                )
            }
        };
        Ok(node)
    }

    pub async fn sync_chains() -> Result<Vec<String>, crate::error::service::ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let app_version = ConfigDomain::get_app_version().await?;
        let chain_list = backend.api_wallet_chain_list(&app_version.app_version).await?;
        ApiChainDomain::upsert_multi_api_chain_than_toggle(chain_list).await
    }

    pub async fn sync_wallet_chain_data(
        wallet_password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        crate::domain::wallet::WalletDomain::validate_password(wallet_password).await?;
        let chain_list: Vec<String> = ApiChainRepo::get_chain_node_list(&pool)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect();

        let account_wallet_mapping =
            ApiAccountRepo::account_wallet_mapping(&pool, Some(ApiWalletType::Withdrawal)).await?;
        let mut req = TokenQueryPriceReq(Vec::new());
        let coins = ApiCoinRepo::coin_list(&pool).await?;

        // let password = ApiWalletDomain::get_passwd().await?;
        let mut api_address_init_req = ApiAddressInitReq::new();
        for wallet in account_wallet_mapping {
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(wallet.account_id)?;

            let seed = ApiWalletDomain::decrypt_seed(wallet_password, &wallet.seed).await?;

            ApiChainDomain::init_chains_api_assets(
                &coins,
                &mut req,
                &mut api_address_init_req,
                &chain_list,
                &seed,
                &account_index_map,
                &wallet.uid,
                &wallet.wallet_address,
                &wallet.account_name,
                false,
                wallet.api_wallet_type,
            )
            .await?;
        }

        // let device_bind_address_task_data =
        //     DeviceDomain::gen_device_bind_address_task_data().await?;
        let api_address_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
            &api_address_init_req,
        )?;
        Tasks::new()
            .push(CommonTask::QueryCoinPrice(req))
            .push(BackendApiTask::BackendApi(api_address_init_task_data))
            // .push(BackendApiTask::BackendApi(expand_address_task_data))
            .send()
            .await?;

        Ok(())
    }
}

pub struct ApiChainTransDomain;

impl ApiChainTransDomain {
    pub async fn assets(
        chain_code: &str,
        from: &str,
        token_address: Option<String>,
    ) -> Result<ApiAssetsEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let assets_id =
            AssetsIdVo { address: from, chain_code: chain_code, token_address: token_address };
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id).await?.ok_or(
            crate::error::business::BusinessError::Assets(
                crate::error::business::assets::AssetsError::NotFound,
            ),
        )?;

        Ok(assets)
    }

    pub async fn main_coin(
        chain_code: &str,
    ) -> Result<ApiCoinEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let coin = ApiCoinRepo::main_coin(chain_code, &pool).await?;
        Ok(coin)
    }
}
