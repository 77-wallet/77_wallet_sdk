use crate::{
    domain::{self, chain::ChainDomain, node::NodeDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, task::Tasks},
};
use wallet_database::{
    entities::node::NodeCreateVo,
    repositories::{
        ResourcesRepo,
        chain::{ChainRepo, ChainRepoTrait},
        node::NodeRepoTrait,
    },
};
pub struct NodeService {
    pub repo: ResourcesRepo,
    // keystore: wallet_crypto::Keystore
}

impl NodeService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
    }

    pub async fn add_node(
        &mut self,
        name: &str,
        chain_code: &str,
        rpc_url: &str,
        _ws_url: &str,
        http_url: Option<String>,
    ) -> Result<String, crate::ServiceError> {
        let tx = &mut self.repo;
        let id = NodeDomain::gen_node_id(name, chain_code);
        let req = NodeCreateVo::new(&id, name, chain_code, rpc_url, http_url);
        let res = NodeRepoTrait::add(tx, req).await.map_err(crate::ServiceError::Database)?;
        Ok(res.node_id)
    }

    async fn init_default_nodes(
        repo: &mut ResourcesRepo,
        chains_set: &mut std::collections::HashSet<(String, String)>,
    ) -> Result<(), crate::ServiceError> {
        let tx = repo;
        let node_list = crate::default_data::node::get_default_node_list()?;
        // let mut default_nodes = Vec::new();
        for (chain_code, nodes) in &node_list.nodes {
            for default_node in nodes.nodes.iter() {
                let key = (default_node.node_name.clone(), chain_code.clone());
                chains_set.insert(key);

                let status = if default_node.active { 1 } else { 0 };

                let id = NodeDomain::gen_node_id(&default_node.node_name, chain_code);
                let node = NodeCreateVo::new(
                    &id,
                    &default_node.node_name,
                    chain_code,
                    &default_node.rpc_url,
                    Some(default_node.http_url.clone()),
                )
                .with_http_url(&default_node.http_url)
                .with_network(&default_node.network)
                .with_status(status)
                .with_is_local(1);
                let _ = match NodeRepoTrait::add(tx, node).await {
                    Ok(node) => node,
                    Err(e) => {
                        tracing::error!("Failed to create default node: {:?}", e);
                        continue;
                    }
                };

                // default_nodes.push(wallet_types::valueobject::NodeData::new(
                //     &node.node_id,
                //     &node.rpc_url,
                //     &node.chain_code,
                // ));
            }
        }
        Ok(())
    }

    pub async fn init_chain_info(&mut self) -> Result<(), crate::ServiceError> {
        let tx = &mut self.repo;
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
        let app_version = domain::app::config::ConfigDomain::get_app_version().await?;

        ChainDomain::toggle_chains(tx, &chain_codes).await?;
        let chain_list_req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::CHAIN_LIST,
            &wallet_transport_backend::request::ChainListReq::new(app_version.app_version),
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(chain_list_req)).send().await?;
        Ok(())
    }

    // 首先在没有请求后端接口的情况下，只需要初始化默认的链信息和节点信息
    // 然后请求后端接口，获取后端默认的链信息和节点信息，然后更新到数据库中
    pub async fn init_node_info(&mut self) -> Result<(), crate::ServiceError> {
        let tx = &mut self.repo;

        let mut chains_set = std::collections::HashSet::new();
        Self::init_default_nodes(tx, &mut chains_set).await?;
        NodeDomain::prune_nodes(tx, &mut chains_set, Some(1)).await?;

        tokio::spawn(async move {
            if let Err(e) = NodeDomain::process_backend_nodes().await {
                tracing::error!("Failed to process default nodes: {:?}", e);
            }
        });

        Ok(())
    }

    pub async fn get_node_list(
        &mut self,
        chain_code: &str,
    ) -> Result<Vec<crate::response_vo::chain::NodeListRes>, crate::ServiceError> {
        let tx = &mut self.repo;

        let Some(chain) = ChainRepoTrait::detail(tx, chain_code).await? else {
            return Err(crate::ServiceError::Business(crate::BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_string()),
            )));
        };

        let node_list =
            NodeRepoTrait::get_node_list_in_chain_codes(tx, &[chain_code.to_string()], Some(1))
                .await?;

        let res = node_list
            .into_iter()
            .map(|node| {
                let status = if chain.node_id == Some(node.node_id.clone()) { 1 } else { 0 };
                crate::response_vo::chain::NodeListRes {
                    node_id: node.node_id,
                    name: node.name,
                    chain_code: node.chain_code,
                    rpc_url: node.rpc_url,
                    ws_url: node.ws_url,
                    status,
                }
            })
            .collect();

        Ok(res)
    }

    // 包括块高、延迟
    pub async fn get_node_dynamic_data(
        &mut self,
        chain_code: &str,
    ) -> Result<Vec<crate::response_vo::chain::NodeDynData>, crate::ServiceError> {
        // let node_list = self.get_node_list(chain_code).await?;
        let tx = &mut self.repo;
        // let list_with_node =
        //     wallet_database::entities::node::NodeEntity::get_node_list_in_chain_codes(
        //         &*pool,
        //         vec![chain_code],
        //     )
        //     .await?;
        let list_with_node =
            NodeRepoTrait::get_node_list_in_chain_codes(tx, &[chain_code.to_string()], Some(1))
                .await?;

        let mut res = Vec::new();
        for node in list_with_node {
            let name = node.name.clone();
            let node_id = node.node_id.clone();
            let chain_instance =
                domain::chain::adapter::ChainAdapterFactory::get_node_transaction_adapter(
                    chain_code,
                    &node.rpc_url,
                )
                .await?;

            let start = std::time::Instant::now();
            let block_height =
                chain_instance.block_num().await.ok().map(|h| h as i64).unwrap_or(-1);
            let delay = (start.elapsed().as_millis() / 2) as u64;
            res.push(crate::response_vo::chain::NodeDynData {
                chain_code: chain_code.to_string(),
                node_id,
                name,
                delay,
                block_height,
            })
        }

        Ok(res)
    }
}
