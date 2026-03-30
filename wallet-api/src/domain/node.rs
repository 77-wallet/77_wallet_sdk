use std::collections::HashMap;

use crate::infrastructure::task_queue::{
    backend::{BackendApiTask, BackendApiTaskData},
    task::Tasks,
};
use wallet_database::{
    CoreDbPool,
    entities::node::NodeCreateVo,
    repositories::{chain::ChainRepo, node::NodeRepo},
};
use wallet_transport_backend::{request::ChainRpcListReq, response_vo::chain::ChainInfos};

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
        let env_network = Self::get_env_network_name();
        let params = vec![name, chain_code, &env_network];
        wallet_utils::snowflake::gen_hash_uid(params)
    }

    pub fn get_env_network_name() -> String {
        if let Ok(ctx) = crate::context::get_context() {
            return ctx.chain_network().as_str().to_owned();
        }

        let mut env = "mainnet".to_owned();
        #[cfg(any(feature = "test", feature = "dev"))]
        {
            env = "testnet".to_owned();
        }
        env
    }

    pub(crate) async fn upsert_chain_rpc(
        pool: &CoreDbPool,
        chain_infos: ChainInfos,
    ) -> Result<(), crate::error::service::ServiceError> {
        for chain_info in chain_infos.list.iter() {
            let network = if chain_info.test { "testnet" } else { "mainnet" };
            let node = NodeCreateVo::new(
                &chain_info.id,
                &chain_info.name,
                &chain_info.chain_code,
                &chain_info.rpc,
                chain_info.http_url.clone(),
            )
            .with_network(network);
            tracing::debug!("创建节点: {:?}", node);
            match NodeRepo::upsert(pool, node).await {
                Ok(node) => tracing::debug!("创建节点成功: {:?}", node),
                Err(e) => {
                    tracing::error!("node_create error: {:?}", e);
                    continue;
                }
            };
        }

        let mut by_chain: HashMap<String, Vec<String>> = HashMap::new();
        for n in chain_infos.list.iter() {
            by_chain.entry(n.chain_code.clone()).or_default().push(n.id.clone());
        }

        for (chain, ids) in by_chain {
            let affected = NodeRepo::disable_backend_not_in(pool, &chain, &ids).await?;
            tracing::debug!("disabled {} backend nodes for chain {}", affected, chain);
        }
        Ok(())
    }

    // async fn load_backend_node() -> Result<
    //     wallet_transport_backend::response_vo::chain::ChainList,
    //     crate::error::service::ServiceError,
    // > {
    //     let app_version = ConfigDomain::get_app_version().await?;
    //     let chain_list_req = BackendApiTaskData::new(
    //         wallet_transport_backend::consts::endpoint::CHAIN_LIST,
    //         &wallet_transport_backend::request::ChainListReq::new(app_version.app_version),
    //     )?;
    //
    //     let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
    //
    //     let backend_chains = backend_api
    //         .post_req_str::<wallet_transport_backend::response_vo::chain::ChainList>(
    //             wallet_transport_backend::consts::endpoint::CHAIN_LIST,
    //             &chain_list_req.body.clone(),
    //         )
    //         .await?;
    //     Ok(backend_chains)
    // }

    pub(crate) async fn init_sync_nodes() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let local_chains = ChainRepo::get_chain_list(&pool).await?;
        let chain_codes: Vec<_> =
            local_chains.iter().map(|chain| chain.chain_code.clone()).collect();

        if !chain_codes.is_empty() {
            // 3. 派发 CHAIN_RPC_LIST 任务
            let req = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                &ChainRpcListReq::new(chain_codes),
            )?;
            Tasks::new().push(BackendApiTask::BackendApi(req)).send().await?;
        }

        Ok(())
    }

    pub async fn init_load_default_nodes() -> Result<(), crate::error::service::ServiceError> {
        let node_list = crate::default_data::node::get_default_node_list()?;
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;

        for (chain_code, nodes) in node_list.nodes.iter() {
            {
                for default_node in nodes.nodes.iter() {
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
                    let r = NodeRepo::upsert(&pool, node).await;
                    tracing::debug!("Created node {}: {:?}", id, r);
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "integration-tests"))]
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
