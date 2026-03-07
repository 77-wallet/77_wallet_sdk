use crate::domain::{self, node::NodeDomain};
use wallet_database::{
    entities::node::NodeCreateVo,
    repositories::{RepoCtx, chain::ChainRepoTrait, node::NodeRepoTrait},
};

pub struct NodeService {
    pub repo: RepoCtx,
    // keystore: wallet_crypto::Keystore
}

impl NodeService {
    pub fn new(repo: RepoCtx) -> Self {
        Self { repo }
    }

    pub async fn add_node(
        &mut self,
        name: &str,
        chain_code: &str,
        rpc_url: &str,
        _ws_url: &str,
        http_url: Option<String>,
    ) -> Result<String, crate::error::service::ServiceError> {
        let tx = &mut self.repo;
        let id = NodeDomain::gen_node_id(name, chain_code);
        let req = NodeCreateVo::new(&id, name, chain_code, rpc_url, http_url);
        let res = NodeRepoTrait::add(tx, req)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;
        Ok(res.node_id)
    }

    // // 首先在没有请求后端接口的情况下，只需要初始化默认的链信息和节点信息
    // // 然后请求后端接口，获取后端默认的链信息和节点信息，然后更新到数据库中
    // pub async fn init_node_info(&mut self) -> Result<(), crate::error::service::ServiceError> {
    //     let tx = &mut self.repo;

    //     let mut chains_set = std::collections::HashSet::new();
    //     Self::init_default_nodes(tx, &mut chains_set).await?;
    //     tracing::debug!("init_default_nodes done chains_set: {:?}", chains_set);
    //     NodeDomain::prune_nodes(tx, &mut chains_set, Some(1)).await?;
    //     if let Err(e) = NodeDomain::process_backend_nodes().await {
    //         tracing::error!("Failed to process default nodes: {:?}", e);
    //     }

    //     Ok(())
    // }

    pub async fn get_node_list(
        &mut self,
        chain_code: &str,
    ) -> Result<
        Vec<crate::response_vo::standard_wallet::chain::NodeListRes>,
        crate::error::service::ServiceError,
    > {
        let tx = &mut self.repo;

        let Some(chain) = ChainRepoTrait::detail(tx, chain_code).await? else {
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::NotFound(chain_code.to_string()),
                ),
            ));
        };

        let node_list =
            NodeRepoTrait::get_node_list_in_chain_codes(tx, &[chain_code.to_string()], Some(1))
                .await?;

        let res = node_list
            .into_iter()
            .map(|node| {
                let status = if chain.node_id == Some(node.node_id.clone()) { 1 } else { 0 };
                crate::response_vo::standard_wallet::chain::NodeListRes {
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
    ) -> Result<
        Vec<crate::response_vo::standard_wallet::chain::NodeDynData>,
        crate::error::service::ServiceError,
    > {
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
                    &node.network,
                )
                .await?;

            let start = std::time::Instant::now();
            let block_height =
                chain_instance.block_num().await.ok().map(|h| h as i64).unwrap_or(-1);
            let delay = (start.elapsed().as_millis() / 2) as u64;
            res.push(crate::response_vo::standard_wallet::chain::NodeDynData {
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
