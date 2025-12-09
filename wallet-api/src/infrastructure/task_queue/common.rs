use wallet_crypto::EncryptedJsonGenerator as _;
use wallet_database::{
    entities::{
        api_wallet::ApiWalletType,
        multisig_queue::QueueTaskEntity,
        node::NodeEntity,
        task_queue::{KnownTaskName, TaskName},
    },
    factory::RepositoryFactory,
    repositories::{
        api_wallet::{chain::ApiChainRepo, wallet::ApiWalletRepo},
        chain::ChainRepoTrait,
    },
};
use wallet_transport_backend::request::TokenQueryPriceReq;
use wallet_types::chain::{address::r#type::AddressType, chain::ChainCode};

use crate::{
    domain::{
        api_wallet::{chain::ApiChainDomain, wallet::ApiWalletDomain},
        chain::ChainDomain,
        multisig::{MultisigDomain, MultisigQueueDomain},
        permission::PermissionDomain,
    },
    error::service::ServiceError,
    infrastructure::task_queue::task::{TaskTrait, task_type::TaskType},
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
    service::coin::CoinService,
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
            CommonTask::EncryptPrivateKey(_) => TaskName::Known(KnownTaskName::EncryptPrivateKey),
        }
    }
    fn get_type(&self) -> TaskType {
        TaskType::Common
    }

    fn get_body(&self) -> Result<Option<String>, ServiceError> {
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
            CommonTask::SyncNodesAndLinkToChains(sync_nodes_and_link_to_chains) => {
                Some(wallet_utils::serde_func::serde_to_string(sync_nodes_and_link_to_chains)?)
            }
            CommonTask::EncryptPrivateKey(encrypt_private_key_task) => {
                Some(wallet_utils::serde_func::serde_to_string(encrypt_private_key_task)?)
            }
        };
        Ok(res)
    }

    async fn execute(&self, _id: &str) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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
                    .collect::<Vec<String>>();
                let api_chain_codes = ApiChainRepo::get_chain_list_all_status(&pool)
                    .await?
                    .into_iter()
                    .map(|chain| chain.chain_code)
                    .collect::<Vec<String>>();
                tracing::info!("sync_nodes_and_link_to_chains chain_codes: {:?}", chain_codes);
                ChainDomain::sync_nodes_and_link_to_chains(&mut repo, &chain_codes, &data).await?;
                ApiChainDomain::sync_nodes_and_link_to_api_chains(
                    &mut repo,
                    &api_chain_codes,
                    &data,
                )
                .await?;
            }
            CommonTask::EncryptPrivateKey(task) => {
                use crate::domain::app::config::ConfigDomain;
                use rand::rngs::OsRng;
                use wallet_crypto::KeystoreJsonGenerator;
                use wallet_database::repositories::api_wallet::account::ApiAccountRepo;
                use wallet_utils::serde_func;

                // 解析私钥

                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                let password = ApiWalletDomain::get_passwd().await?;
                let api_wallet = ApiWalletRepo::find_by_address(&pool, &task.wallet_address)
                    .await?
                    .ok_or(crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                    ))?;
                let seed = ApiWalletDomain::decrypt_seed(&password, &api_wallet.seed).await?;
                let code: ChainCode = task.chain_code.as_str().try_into()?;

                let node = ChainDomain::get_node(&task.chain_code).await?;

                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &task.address_type, node.network.as_str().into()).try_into()?;

                let account_index_map =
                    wallet_utils::address::AccountIndexMap::from_account_id(task.account_index)?;

                let keypair = instance
                    .gen_keypair_with_index_address_type(&seed, account_index_map.input_index)
                    .map_err(|e| crate::error::system::SystemError::Service(e.to_string()))?;

                let private_key = keypair.private_key()?;
                let private_key = wallet_utils::serde_func::serde_to_vec(&private_key)?;
                // 加密私钥
                let algorithm = ConfigDomain::get_keystore_kdf_algorithm().await?;
                let rng = OsRng;
                let mut generator = KeystoreJsonGenerator::new(rng, algorithm.clone());
                let encrypted_private_key =
                    generator.generate(task.wallet_password.as_bytes(), &private_key)?;
                let encrypted_private_key_str =
                    serde_func::serde_to_string(&encrypted_private_key)?;

                // 更新数据库中的私钥
                ApiAccountRepo::update_private_key(
                    &pool,
                    &task.address,
                    &encrypted_private_key_str,
                )
                .await?;
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
    EncryptPrivateKey(EncryptPrivateKeyTask),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RecoverDataBody {
    pub uid: String,
    // 波场恢复权限使用的地址
    pub tron_address: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EncryptPrivateKeyTask {
    pub address: String,
    pub address_type: AddressType,
    pub account_index: u32,
    pub wallet_password: String,
    pub wallet_address: String,
    pub chain_code: String,
    pub api_wallet_type: ApiWalletType,
}
impl RecoverDataBody {
    pub fn new(uid: &str) -> Self {
        Self { uid: uid.to_string(), tron_address: None }
    }
}
