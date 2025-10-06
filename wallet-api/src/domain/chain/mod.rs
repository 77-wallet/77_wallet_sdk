pub mod adapter;
pub mod swap;
pub mod transaction;

use std::collections::HashSet;

use super::{account::AccountDomain, assets::AssetsDomain, wallet::WalletDomain};
use crate::{
    domain::app::config::ConfigDomain,
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    response_vo,
};
use wallet_chain_interact::{
    BillResourceConsume, btc::ParseBtcAddress, dog::ParseDogAddress, eth::FeeSetting,
    ltc::ParseLtcAddress, ton::address::parse_addr_from_bs64_url,
};
use wallet_database::{
    entities::{coin::CoinEntity, node::NodeEntity},
    repositories::{
        ResourcesRepo, TransactionTrait as _,
        account::AccountRepo,
        chain::ChainRepo,
        node::{NodeRepo, NodeRepoTrait},
    },
};
use wallet_transport_backend::request::{AddressBatchInitReq, ChainRpcListReq, TokenQueryPriceReq};
use wallet_types::chain::{
    chain::ChainCode,
    network::{self, NetworkKind},
};
use wallet_utils::address;

pub struct TransferResp {
    pub tx_hash: String,
    pub fee: String,
    pub consumer: Option<BillResourceConsume>,
}
impl TransferResp {
    pub fn new(tx_hash: String, fee: String) -> Self {
        Self { tx_hash, fee, consumer: None }
    }
    pub fn with_consumer(&mut self, consumer: BillResourceConsume) {
        self.consumer = Some(consumer);
    }

    pub fn resource_consume(&self) -> Result<String, crate::error::service::ServiceError> {
        if let Some(consumer) = &self.consumer {
            Ok(consumer.to_json_str()?)
        } else {
            Ok(String::new())
        }
    }
}

/// Parses a fee setting string into a `FeeSetting` struct.
pub fn pare_fee_setting(
    fee_setting: &str,
) -> Result<FeeSetting, crate::error::service::ServiceError> {
    fee_setting.try_into().and_then(|s: response_vo::EthereumFeeDetails| FeeSetting::try_from(s))
}

pub fn rpc_need_header(_url: &str) -> Result<bool, crate::error::service::ServiceError> {
    // let url = Url::parse(url).expect("Invalid URL");
    // Ok(url.host_str() == Some(wallet_transport_backend::consts::BASE_RPC_URL))
    Ok(true)
}

pub fn check_address(
    address: &str,
    chain: wallet_types::chain::chain::ChainCode,
    network: network::NetworkKind,
) -> Result<(), crate::error::service::ServiceError> {
    match chain {
        wallet_types::chain::chain::ChainCode::Bitcoin => {
            let parse = ParseBtcAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::BnbSmartChain
        | wallet_types::chain::chain::ChainCode::Ethereum => {
            wallet_utils::address::parse_eth_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::Tron => {
            if wallet_utils::address::is_tron_address(address) {
                true
            } else {
                return Err(crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                ))?;
            }
        }
        wallet_types::chain::chain::ChainCode::Solana => {
            wallet_utils::address::parse_sol_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::Ton => {
            parse_addr_from_bs64_url(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::Litecoin => {
            let parse = ParseLtcAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::Dogcoin => {
            let parse = ParseDogAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
        wallet_types::chain::chain::ChainCode::Sui => {
            address::parse_sui_address(address).map(|_| true).map_err(|_| {
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::AddressNotCorrect,
                )
            })?
        }
    };
    Ok(())
}

pub struct ChainDomain;

impl ChainDomain {
    pub(crate) async fn upsert_multi_chain_than_toggle(
        chains: wallet_transport_backend::response_vo::chain::ChainList,
    ) -> Result<bool, crate::error::service::ServiceError> {
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
        let mut has_new_chain = false;
        let account_list = AccountRepo::list(&pool).await?;
        let app_version = super::app::config::ConfigDomain::get_app_version().await?.app_version;
        for chain in chains.list {
            let Some(master_token_code) = chain.master_token_code else {
                continue;
            };

            let status = match (
                super::app::config::ConfigDomain::compare_versions(
                    &app_version,
                    &chain.app_version_code,
                ),
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
                has_new_chain = true;
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
                wallet_database::entities::chain::ChainCreateVo::new(
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

        ChainRepo::upsert_multi_chain(&pool, input).await?;
        Self::toggle_chains(&chain_codes).await?;

        if !chain_codes.is_empty() {
            let chain_rpc_list_req = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                &ChainRpcListReq::new(chain_codes),
            )?;
            Tasks::new().push(BackendApiTask::BackendApi(chain_rpc_list_req)).send().await?;
        }

        Ok(has_new_chain)
    }

    pub(crate) async fn toggle_chains(
        chain_codes: &[String],
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ChainRepo::toggle_chains_status(&pool, chain_codes).await?;
        Ok(())
    }

    // pub(crate) async fn chain_node_info_left_join(
    //     repo: &mut wallet_database::repositories::ResourcesRepo,
    //     chain_code: &str,
    // ) -> Result<Option<ChainWithNode>, crate::ServiceError> {
    //     let chain = repo.chain_node_info_left_join(chain_code).await?;
    //     if let Some(chain) = chain {
    //         if chain.node_id.is_empty() {
    //             let existing_nodes = NodeRepoTrait::list(repo, Some(1)).await?;
    //             let existing_node = existing_nodes
    //                 .into_iter()
    //                 .find(|node| node.chain_code == chain_code);

    //             let chain = ChainRepoTrait::detail(repo, chain_code).await?;
    //             if let Some(chain) = chain
    //                 && let Some(existing_node) = existing_node
    //             {
    //                 Ok(Some((chain, existing_node).into()))
    //             } else {
    //                 Ok(None)
    //             }
    //         } else {
    //             Ok(Some(chain))
    //         }
    //     } else {
    //         Ok(None)
    //     }
    // }

    pub(crate) async fn get_node(
        chain_code: &str,
    ) -> Result<NodeInfo, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let node = match ChainRepo::detail_with_node(&pool, chain_code).await? {
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

    pub(crate) async fn init_chains_assets(
        tx: &mut ResourcesRepo,
        coins: &[CoinEntity],
        req: &mut TokenQueryPriceReq,
        address_batch_init_task_data: &mut AddressBatchInitReq,
        subkeys: &mut Vec<wallet_tree::file_ops::BulkSubkey>,
        chain_list: &[String],
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        derivation_path: Option<&str>,
        uid: &str,
        wallet_address: &str,
        account_name: &str,
        is_default_name: bool,
    ) -> Result<(), crate::error::service::ServiceError> {
        for chain in chain_list.iter() {
            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = Self::get_node(chain).await else {
                continue;
            };

            for address_type in address_types {
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &address_type, node.network.as_str().into()).try_into()?;
                // (&code, &address_type, "mainnet".into()).try_into()?;

                let (account_address, derivation_path, address_init_req) =
                    AccountDomain::create_account_v2(
                        tx,
                        seed,
                        &instance,
                        derivation_path,
                        account_index_map,
                        uid,
                        wallet_address,
                        account_name,
                        is_default_name,
                    )
                    .await?;

                if let Some(address_init_req) = address_init_req {
                    address_batch_init_task_data.0.push(address_init_req);
                } else {
                    tracing::info!("不上报： {}", account_address.address);
                };

                subkeys.push(
                    AccountDomain::generate_subkey(
                        &instance,
                        seed,
                        &account_address.address,
                        &code.to_string(),
                        account_index_map,
                        derivation_path.as_str(),
                    )
                    .await?,
                );
                AssetsDomain::init_default_assets(
                    coins,
                    &account_address.address,
                    &code.to_string(),
                    req,
                )
                .await?;
            }
        }
        Ok(())
    }

    pub(crate) fn check_token_address(
        token_address: &mut String,
        chain_code: &str,
        net: NetworkKind,
    ) -> Result<(), crate::error::service::ServiceError> {
        let chain: wallet_types::chain::chain::ChainCode = chain_code.try_into()?;

        match chain {
            wallet_types::chain::chain::ChainCode::Ethereum
            | wallet_types::chain::chain::ChainCode::BnbSmartChain => {
                *token_address = wallet_utils::address::to_checksum_address(token_address);
            }
            _ => {}
        }

        match chain {
            wallet_types::chain::chain::ChainCode::Sui => {
                wallet_utils::address::parse_sui_type_tag(token_address).map_err(|_| {
                    crate::error::business::BusinessError::Account(
                        crate::error::business::account::AccountError::AddressNotCorrect,
                    )
                })?;
            }
            _ => check_address(token_address, chain, net)?,
        }
        Ok(())
    }

    pub async fn init_chain_info() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let list = crate::default_data::chain::get_default_chains_list()?;

        // tracing::warn!("list {:#?}", list);

        let mut chain_codes = Vec::new();
        for (chain_code, default_chain) in &list.chains {
            let status = if default_chain.active { 1 } else { 0 };
            // let node_id =
            //     NodeDomain::gen_node_id(&default_chain.node_name, &default_chain.chain_code);
            let req = wallet_database::entities::chain::ChainCreateVo::new(
                &default_chain.name,
                &default_chain.chain_code,
                &default_chain.protocols,
                &default_chain.main_symbol,
            )
            .with_status(status);

            if let Err(e) = ChainRepo::add(&pool, req).await {
                tracing::error!("Failed to create default chain: {:?}", e);
                continue;
            }
            if status == 1 {
                chain_codes.push(chain_code.to_string());
            }
        }
        let app_version = ConfigDomain::get_app_version().await?;

        ChainDomain::toggle_chains(&chain_codes).await?;
        let chain_list_req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::CHAIN_LIST,
            &wallet_transport_backend::request::ChainListReq::new(app_version.app_version),
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(chain_list_req)).send().await?;
        Ok(())
    }

    // 为缺少节点的链分配节点，同时也包含了同步和过滤节点的操作
    pub(crate) async fn sync_nodes_and_link_to_chains(
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
                Self::set_chain_node(repo, backend_nodes, &node.chain_code).await?;
            }
        }
        Self::assign_missing_nodes_to_chains(backend_nodes).await?;
        Ok(())
    }

    /// 设置链使用的节点
    pub(crate) async fn set_chain_node(
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
                ChainRepo::set_chain_node(&pool, chain_code, &backend_nodes.node_id).await
            {
                tracing::error!("set_chain_node error: {:?}", e);
            }
        } else if let Some(node) = default_nodes.iter().find(|node| node.chain_code == chain_code) {
            if let Err(e) = ChainRepo::set_chain_node(&pool, chain_code, &node.node_id).await {
                tracing::error!("set_chain_node error: {:?}", e);
            }
        }
        repo.commit_transaction().await?;
        Ok(())
    }

    pub(crate) async fn assign_missing_nodes_to_chains(
        backend_nodes: &[NodeEntity],
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let chain_list = ChainRepo::get_chain_list(&pool).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        for chain in chain_list {
            if chain.node_id.is_none() {
                tracing::debug!(
                    "[assign_missing_nodes_to_chains] set chain node: {}",
                    chain.chain_code
                );
                Self::set_chain_node(&mut repo, backend_nodes, &chain.chain_code).await?;
            }
        }
        tracing::debug!("[assign_missing_nodes_to_chains] end");
        Ok(())
    }
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub chain_code: String,
    pub node_id: String,
    pub node_name: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub http_url: String,
    pub network: String,
    pub status: u8,
}

impl NodeInfo {
    pub fn new(
        chain_code: &str,
        node_id: &str,
        node_name: &str,
        rpc_url: &str,
        ws_url: &str,
        http_url: &str,
        network: &str,
        status: u8,
    ) -> Self {
        Self {
            chain_code: chain_code.to_string(),
            node_id: node_id.to_string(),
            node_name: node_name.to_string(),
            rpc_url: rpc_url.to_string(),
            ws_url: ws_url.to_string(),
            http_url: http_url.to_string(),
            network: network.to_string(),
            status,
        }
    }
}
