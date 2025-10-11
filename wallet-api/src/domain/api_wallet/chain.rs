use std::collections::HashSet;

use wallet_database::{
    entities::{api_wallet::ApiWalletType, coin::CoinEntity, node::NodeEntity},
    repositories::{
        ResourcesRepo, TransactionTrait as _,
        api_wallet::{account::ApiAccountRepo, chain::ApiChainRepo, wallet::ApiWalletRepo},
        node::NodeRepoTrait,
    },
};
use wallet_transport_backend::request::{
    ChainRpcListReq, TokenQueryPriceReq, api_wallet::address::ApiAddressInitReq,
};
use wallet_types::chain::chain::ChainCode;

use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        assets::AssetsDomain,
        chain::ChainDomain,
        wallet::WalletDomain,
    },
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
};

pub struct ApiChainDomain {}

impl ApiChainDomain {
    pub(crate) async fn init_chains_api_assets(
        coins: &[CoinEntity],
        req: &mut TokenQueryPriceReq,
        api_address_init_req: &mut ApiAddressInitReq,
        // expand_address_req: &mut AddressBatchInitReq,
        // subkeys: &mut Vec<wallet_tree::file_ops::BulkSubkey>,
        chain_list: &[String],
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        for chain in chain_list.iter() {
            // let index = account_index_map.input_index;
            // let mut params = AddressParam::new(index);

            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = ChainDomain::get_node(chain).await else {
                continue;
            };

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
                    wallet_password,
                    api_wallet_type,
                )
                .await?;

                if let Some(address_init_req) = address_init_req {
                    api_address_init_req.address_list.add_address(address_init_req);
                    // params.push(&account_address.address);
                } else {
                    tracing::info!("不上报： {}", account_address);
                };

                // subkeys.push(
                //     AccountDomain::generate_subkey(
                //         &instance,
                //         seed,
                //         &account_address.address,
                //         &code.to_string(),
                //         account_index_map,
                //         derivation_path.as_str(),
                //     )
                //     .await?,
                // );

                AssetsDomain::init_default_api_assets(
                    coins,
                    &account_address,
                    &code.to_string(),
                    req,
                )
                .await?;
            }

            // if !params.address_list.is_empty() {
            //     expand_address_req.add_chain_code(chain, params);
            // }
        }

        Ok(())
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
        // 本地的backend_nodes 和 backend_nodes 比较，把backend_nodes中没有，local_backend_nodes有的节点，删除
        let local_backend_nodes = NodeRepoTrait::list_by_chain(repo, &chain_code, Some(0)).await?;
        let backend_node_rpcs: HashSet<String> = backend_nodes
            .iter()
            .filter(|node| chain_code.contains(&node.chain_code))
            .map(|n| n.node_id.clone())
            .collect();

        for node in local_backend_nodes {
            if !backend_node_rpcs.contains(&node.node_id) {
                if let Err(e) = NodeRepoTrait::delete(repo, &node.node_id).await {
                    tracing::error!("Failed to remove filtered node {}: {:?}", node.node_id, e);
                }
                Self::set_api_chain_node(repo, backend_nodes, &node.chain_code).await?;
            }
        }
        Self::assign_missing_nodes_to_api_chains(backend_nodes).await?;
        Ok(())
    }

    pub(crate) async fn set_api_chain_node(
        repo: &mut ResourcesRepo,
        backend_nodes: &[NodeEntity],
        // default_nodes: &[NodeData],
        chain_code: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let list = NodeRepoTrait::list(repo, Some(1)).await?;

        let mut default_nodes = Vec::new();
        for default_node in list.iter() {
            // let node_id = NodeDomain::gen_node_id(&default_node.name, &default_node.chain_code);
            default_nodes.push(wallet_types::valueobject::NodeData::new(
                &default_node.node_id,
                &default_node.rpc_url,
                &default_node.chain_code,
            ));
        }

        repo.begin_transaction().await?;
        if let Some(backend_nodes) = backend_nodes.iter().find(|node| node.chain_code == chain_code)
        {
            if let Err(e) =
                ApiChainRepo::set_api_chain_node(&pool, chain_code, &backend_nodes.node_id).await
            {
                tracing::error!("set_api_chain_node error: {:?}", e);
            }
        } else if let Some(node) = default_nodes.iter().find(|node| node.chain_code == chain_code) {
            if let Err(e) = ApiChainRepo::set_api_chain_node(&pool, chain_code, &node.node_id).await
            {
                tracing::error!("set_api_chain_node error: {:?}", e);
            }
        }
        repo.commit_transaction().await?;
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
            ApiWalletRepo::list(&pool, Some(ApiWalletType::Withdrawal)).await?;

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
}
