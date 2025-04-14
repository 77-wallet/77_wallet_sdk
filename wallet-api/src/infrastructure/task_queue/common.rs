use wallet_database::{
    entities::{multisig_queue::QueueTaskEntity, node::NodeEntity, task_queue::TaskName},
    factory::RepositoryFactory,
    repositories::chain::ChainRepoTrait,
    DbPool,
};
use wallet_transport_backend::request::TokenQueryPriceReq;

use crate::{
    domain::{
        multisig::{MultisigDomain, MultisigQueueDomain},
        node::NodeDomain,
        permission::PermissionDomain,
    },
    service::coin::CoinService,
};

pub(crate) enum CommonTask {
    QueryCoinPrice(TokenQueryPriceReq),
    QueryQueueResult(QueueTaskEntity),
    RecoverMultisigAccountData(RecoverDataBody),
    SyncNodesAndLinkToChains(Vec<NodeEntity>),
}
impl CommonTask {
    pub(crate) fn get_name(&self) -> TaskName {
        match self {
            CommonTask::QueryCoinPrice(_) => TaskName::QueryCoinPrice,
            CommonTask::QueryQueueResult(_) => TaskName::QueryQueueResult,
            CommonTask::RecoverMultisigAccountData(_) => TaskName::RecoverMultisigAccountData,
            CommonTask::SyncNodesAndLinkToChains(_) => TaskName::SyncNodesAndLinkToChains,
        }
    }

    pub(crate) fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
        let res = match self {
            CommonTask::QueryCoinPrice(query_coin_price) => {
                Some(wallet_utils::serde_func::serde_to_string(query_coin_price)?)
            }
            CommonTask::QueryQueueResult(queue) => {
                Some(wallet_utils::serde_func::serde_to_string(queue)?)
            }
            CommonTask::RecoverMultisigAccountData(recover_data) => {
                Some(wallet_utils::serde_func::serde_to_string(recover_data)?)
            }
            // CommonTask::RecoverPermission(uid) => Some(uid.to_string()),
            CommonTask::SyncNodesAndLinkToChains(sync_nodes_and_link_to_chains) => Some(
                wallet_utils::serde_func::serde_to_string(sync_nodes_and_link_to_chains)?,
            ),
        };
        Ok(res)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RecoverDataBody {
    pub uid: String,
    // 波场恢复权限使用的地址
    pub tron_address: Option<String>,
}
impl RecoverDataBody {
    pub fn new(uid: &str) -> Self {
        Self {
            uid: uid.to_string(),
            tron_address: None,
        }
    }
}

pub(crate) async fn handle_common_task(
    task: CommonTask,
    pool: DbPool,
) -> Result<(), crate::ServiceError> {
    match task {
        CommonTask::QueryCoinPrice(data) => {
            let repo = RepositoryFactory::repo(pool.clone());
            let coin_service = CoinService::new(repo);
            coin_service.query_token_price(data).await?;
        }
        CommonTask::QueryQueueResult(data) => {
            MultisigQueueDomain::sync_queue_status(&data.id).await?
        }
        CommonTask::RecoverMultisigAccountData(body) => {
            MultisigDomain::recover_uid_multisig_data(&body.uid, None).await?;
            if let Some(address) = &body.tron_address {
                PermissionDomain::recover_permission(vec![address.clone()]).await?;
            }

            MultisigQueueDomain::recover_all_queue_data(&body.uid).await?;
        }
        CommonTask::SyncNodesAndLinkToChains(data) => {
            let mut repo = RepositoryFactory::repo(pool.clone());
            let chain_codes = ChainRepoTrait::get_chain_list_all_status(&mut repo)
                .await?
                .into_iter()
                .map(|chain| chain.chain_code)
                .collect();
            NodeDomain::sync_nodes_and_link_to_chains(&mut repo, chain_codes, &data).await?;
        }
    }
    Ok(())
}
