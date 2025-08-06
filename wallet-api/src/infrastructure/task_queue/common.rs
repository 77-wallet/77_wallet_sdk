use wallet_database::{
    entities::{
        multisig_queue::QueueTaskEntity,
        node::NodeEntity,
        task_queue::{KnownTaskName, TaskName},
    },
    factory::RepositoryFactory,
    repositories::chain::ChainRepoTrait,
};
use wallet_transport_backend::request::TokenQueryPriceReq;

use crate::{
    domain::{
        multisig::{MultisigDomain, MultisigQueueDomain},
        node::NodeDomain,
        permission::PermissionDomain,
    },
    infrastructure::task_queue::task::{task_type::TaskType, TaskTrait},
    service::coin::CoinService,
    FrontendNotifyEvent, NotifyEvent,
};

#[async_trait::async_trait]
impl TaskTrait for CommonTask {
    fn get_name(&self) -> TaskName {
        match self {
            CommonTask::QueryCoinPrice(_) => TaskName::Known(KnownTaskName::QueryCoinPrice),
            CommonTask::QueryQueueResult(_) => TaskName::Known(KnownTaskName::QueryQueueResult),
            CommonTask::RecoverMultisigAccountData(_) => {
                TaskName::Known(KnownTaskName::RecoverMultisigAccountData)
            }
            CommonTask::SyncNodesAndLinkToChains(_) => {
                TaskName::Known(KnownTaskName::SyncNodesAndLinkToChains)
            }
        }
    }
    fn get_type(&self) -> TaskType {
        TaskType::Common
    }
    fn get_body(&self) -> Result<Option<String>, crate::ServiceError> {
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

    async fn execute(&self, _id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        match self {
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

                // 恢复完成后发送事件给前端
                let data = NotifyEvent::RecoverComplete;
                FrontendNotifyEvent::new(data).send().await?;
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) enum CommonTask {
    QueryCoinPrice(TokenQueryPriceReq),
    QueryQueueResult(QueueTaskEntity),
    RecoverMultisigAccountData(RecoverDataBody),
    SyncNodesAndLinkToChains(Vec<NodeEntity>),
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
