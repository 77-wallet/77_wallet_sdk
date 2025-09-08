use std::collections::HashSet;

use wallet_database::{
    entities::node::{NodeCreateVo, NodeEntity},
    repositories::{chain::ChainRepoTrait, node::NodeRepoTrait, ResourcesRepo, TransactionTrait},
};
use wallet_transport_backend::{request::ChainRpcListReq, response_vo::chain::ChainInfos};

use crate::infrastructure::task_queue::{
    task::Tasks, BackendApiTask, BackendApiTaskData, CommonTask,
};

pub struct NodeDomain;

impl NodeDomain {
    // pub(crate) async fn ping_node(node_id: &str)-> Result<(), crate::ServiceError> {
    //     let backend = crate::manager::Context::get_global_backend_api().unwrap();
    //     surge_ping::ping(host, payload)
    //     ping::ping(addr, timeout, ttl, ident, seq_cnt, payload)
    //     let res = backend.ping_node(node_id).await;
    //     match res {
    //         Ok(_) => Ok(()),
    //         Err(e) => {
    //             tracing::error!("ping_node: {:?}", e);
    //             Err(crate::ServiceError::Business(crate::BusinessError::Node(
    //                 crate::NodeError::PingFailed(node_id.to_string()),
    //             )))
    //         }
    //     }
    // }

    pub(crate) fn gen_node_id(name: &str, chain_code: &str) -> String {
        let params = vec![name, chain_code];
        wallet_utils::snowflake::gen_hash_uid(params)
    }

    pub(crate) async fn upsert_chain_rpc(
        repo: &mut ResourcesRepo,
        nodes: ChainInfos,
        backend_nodes: &mut Vec<NodeEntity>,
    ) -> Result<(), crate::ServiceError> {
        for node in nodes.list.iter() {
            let network = if node.test { "testnet" } else { "mainnet" };
            let node = NodeCreateVo::new(
                &node.id,
                &node.name,
                &node.chain_code,
                &node.rpc,
                node.http_url.clone(),
            )
            .with_network(network);
            tracing::debug!("创建节点: {:?}", node);
            match NodeRepoTrait::add(repo, node).await {
                Ok(node) => backend_nodes.push(node),
                Err(e) => {
                    tracing::error!("node_create error: {:?}", e);
                    continue;
                }
            };
        }

        Ok(())
    }

    // 本地默认的链和后端请求的链两边分开去处理
    // 1.遍历本地默认的链，如去后端请求获取每个链的节点，如果请求成功，则更新节点表的节点，
    // 如果请求失败，将这个链的请求发送到任务队列去重试，任务队列重复执行该请求，直到成功，并更新节点表的节点
    // 2.请求后端获取链列表，同样的，请求成功则更新链表的链数据，请求失败则发送到任务队列去重试，直到成功，并更新链表的信息
    // 请求成功的话，遍历链列表，执行1的操作
    pub(crate) async fn process_backend_nodes() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let backend = crate::manager::Context::get_global_backend_api()?;

        let local_chains = ChainRepoTrait::get_chain_list_all_status(&mut repo)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect::<Vec<String>>();
        let mut backend_nodes = Vec::new();

        match backend
            .chain_rpc_list(ChainRpcListReq::new(local_chains.clone()))
            .await
        {
            Ok(nodes) => {
                Self::upsert_chain_rpc(&mut repo, nodes, &mut backend_nodes).await?;

                Tasks::new()
                    .push(CommonTask::SyncNodesAndLinkToChains(backend_nodes))
                    .send()
                    .await?;
            }
            Err(e) => {
                tracing::error!("backend get chain rpc list error: {:?}", e);
                let chain_rpc_list_req = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                    &ChainRpcListReq::new(local_chains),
                )?;
                Tasks::new()
                    .push(BackendApiTask::BackendApi(chain_rpc_list_req))
                    .send()
                    .await?;
            }
        };

        Ok(())
    }

    /// 从表中移除不在给定链集合中的节点，并在删除节点时处理相关链的设置
    pub(crate) async fn prune_nodes(
        repo: &mut ResourcesRepo,
        chains_set: &mut std::collections::HashSet<(String, String)>,
        is_local: Option<u8>,
    ) -> Result<(), crate::ServiceError> {
        let tx = repo;
        let existing_nodes = NodeRepoTrait::list(tx, is_local).await?;

        for node in existing_nodes {
            let key = (node.name.clone(), node.chain_code.clone());
            if !chains_set.contains(&key) {
                match NodeRepoTrait::delete(tx, &node.node_id).await {
                    Ok(node) => node,
                    Err(e) => {
                        tracing::error!("Failed to remove filtered node {}: {:?}", node.node_id, e);
                        continue;
                    }
                };
                // 将链表中有设置该节点的行的node_id设置为空
                ChainRepoTrait::set_chain_node_id_empty(tx, &node.node_id).await?;
            }
        }
        Ok(())
    }

    // 为缺少节点的链分配节点，同时也包含了同步和过滤节点的操作
    pub(crate) async fn sync_nodes_and_link_to_chains(
        repo: &mut ResourcesRepo,
        chain_code: Vec<String>,
        backend_nodes: &[NodeEntity],
    ) -> Result<(), crate::ServiceError> {
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
        Self::assign_missing_nodes_to_chains(repo, backend_nodes).await?;
        Ok(())
    }

    pub(crate) async fn assign_missing_nodes_to_chains(
        repo: &mut ResourcesRepo,
        backend_nodes: &[NodeEntity],
    ) -> Result<(), crate::ServiceError> {
        let chain_list = ChainRepoTrait::get_chain_list(repo).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
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

    /// 设置链使用的节点
    pub(crate) async fn set_chain_node(
        repo: &mut ResourcesRepo,
        backend_nodes: &[NodeEntity],
        // default_nodes: &[NodeData],
        chain_code: &str,
    ) -> Result<(), crate::ServiceError> {
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
        if let Some(backend_nodes) = backend_nodes
            .iter()
            .find(|node| node.chain_code == chain_code)
        {
            if let Err(e) =
                ChainRepoTrait::set_chain_node(repo, chain_code, &backend_nodes.node_id).await
            {
                tracing::error!("set_chain_node error: {:?}", e);
            }
        } else if let Some(node) = default_nodes
            .iter()
            .find(|node| node.chain_code == chain_code)
        {
            if let Err(e) = ChainRepoTrait::set_chain_node(repo, chain_code, &node.node_id).await {
                tracing::error!("set_chain_node error: {:?}", e);
            }
        }
        repo.commit_transaction().await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Instant;

    #[tokio::test]
    async fn main() {
        let url = "https://rpc.ankr.com/premium-http/tron/2554129db2045e61c3b8584ad6ee32b7b7808916d160e3e16b51dfee6d17d56c";

        let start = Instant::now();
        match reqwest::get(url).await {
            Ok(response) => {
                let duration = start.elapsed();
                println!(
                    "Ping successful! Status: {}, Time: {:?}",
                    response.status(),
                    duration
                );
            }
            Err(err) => {
                println!("Ping failed: {:?}", err);
            }
        }
    }
}
