use wallet_database::{
    entities::node::{NodeCreateVo, NodeEntity},
    repositories::{
        ResourcesRepo, api_chain::ApiChainRepo, chain::ChainRepoTrait, node::NodeRepoTrait,
    },
};
use wallet_transport_backend::{request::ChainRpcListReq, response_vo::chain::ChainInfos};

use crate::infrastructure::task_queue::{
    CommonTask,
    backend::{BackendApiTask, BackendApiTaskData},
    task::Tasks,
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
    ) -> Result<(), crate::error::service::ServiceError> {
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
    pub(crate) async fn process_backend_nodes() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let local_chains = ChainRepoTrait::get_chain_list_all_status(&mut repo)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect::<Vec<String>>();
        let mut backend_nodes = Vec::new();

        if local_chains.is_empty() {
            return Ok(());
        }

        match backend.chain_rpc_list(ChainRpcListReq::new(local_chains.clone())).await {
            Ok(nodes) => {
                Self::upsert_chain_rpc(&mut repo, nodes, &mut backend_nodes).await?;

                Tasks::new()
                    .push(CommonTask::SyncNodesAndLinkToChains(backend_nodes))
                    .send()
                    .await?;
            }
            Err(e) => {
                tracing::error!("backend get chain rpc list error: {:?}", e);
                if local_chains.is_empty() {
                    return Ok(());
                }
                let chain_rpc_list_req = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                    &ChainRpcListReq::new(local_chains),
                )?;
                Tasks::new().push(BackendApiTask::BackendApi(chain_rpc_list_req)).send().await?;
            }
        };

        Ok(())
    }

    /// 从表中移除不在给定链集合中的节点，并在删除节点时处理相关链的设置
    pub(crate) async fn prune_nodes(
        repo: &mut ResourcesRepo,
        chains_set: &mut std::collections::HashSet<(String, String)>,
        is_local: Option<u8>,
    ) -> Result<(), crate::error::service::ServiceError> {
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
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                ApiChainRepo::set_chain_node_id_empty(&pool, &node.node_id).await?;
            }
        }
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
                println!("Ping successful! Status: {}, Time: {:?}", response.status(), duration);
            }
            Err(err) => {
                println!("Ping failed: {:?}", err);
            }
        }
    }
}
