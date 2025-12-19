// Actor-based expand address manager for your wallet system.
// - Supervisor manages per-(uid,chain) ExpandActor
// - Each ExpandActor runs in a single tokio task and serializes all operations
// - On startup, supervisor can recover unfinished tasks from TaskQueueRepo
// - ADDRESS_INIT events and incoming expand tasks are sent to the actor

use once_cell::sync::Lazy;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use tokio::{
    spawn,
    sync::{Mutex, mpsc, oneshot},
};
use wallet_database::{
    entities::api_wallet::ApiWalletEntity,
    repositories::api_wallet::{
        account::ApiAccountRepo, expand_batch::ExpandBatchRepo,
        expand_batch_item::ExpandBatchItemRepo, wallet::ApiWalletRepo,
    },
};

use crate::{
    context::CONTEXT,
    domain::api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AwmCmdAddrExpandMsg,
};

use crate::error::service::ServiceError;
use wallet_transport_backend::request::{
    AddressInitReq,
    api_wallet::address::{ApiAddressInitReq, ExpandAddressCompleteReq},
};

// size of internal channels
const SUPERVISOR_CHANNEL_SIZE: usize = 512;
const ACTOR_CHANNEL_SIZE: usize = 256;

// key for actor map
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ActorKey {
    uid: String,
    chain: String,
}

impl From<(&str, &str)> for ActorKey {
    fn from((u, c): (&str, &str)) -> Self {
        Self { uid: u.to_string(), chain: c.to_string() }
    }
}

// Messages the actor understands
#[derive(Debug)]
pub enum ExpandActorMsg {
    /// New expand task arrives (task_id optional, msg struct comes from TaskQueue)
    NewExpandTask {
        task_id: String,
        msg: AwmCmdAddrExpandMsg,
        reply: Option<oneshot::Sender<Result<(), ServiceError>>>,
    },
    /// Address inited from ADDRESS_INIT handler
    AddressInited {
        indices: Vec<i32>, // 支持多个索引
    },
    /// Recover existing task (used on startup)
    RecoverTask {
        task_id: String,
        batch_id: String,
        reply: Option<oneshot::Sender<Result<(), ServiceError>>>,
    },
    /// Shutdown actor
    Shutdown,
}

#[derive(Clone)]
pub struct ExpandActorHandle {
    sender: mpsc::Sender<ExpandActorMsg>,
}

impl ExpandActorHandle {
    pub async fn send(&self, msg: ExpandActorMsg) -> Result<(), ServiceError> {
        self.sender.send(msg).await.map_err(|_| {
            ServiceError::System(crate::error::system::SystemError::Internal("actor closed".into()))
        })
    }
}

// Supervisor which holds actor handles
type ActorMap = Arc<Mutex<HashMap<ActorKey, ExpandActorHandle>>>;

static SUPERVISOR: Lazy<ActorMap> = Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Get or create an actor for (uid, chain). This will spawn the actor task if necessary.
pub async fn get_or_create_actor(
    uid: &str,
    chain: &str,
) -> Result<ExpandActorHandle, crate::error::service::ServiceError> {
    let key = ActorKey::from((uid, chain));
    let mut map = SUPERVISOR.lock().await;
    if let Some(handle) = map.get(&key) {
        return Ok(handle.clone());
    }

    let (tx, rx) = mpsc::channel(ACTOR_CHANNEL_SIZE);
    let handle = ExpandActorHandle { sender: tx.clone() };

    // spawn actor
    let uid_c = uid.to_string();
    let chain_c = chain.to_string();

    let actor = ExpandActor::new(uid_c.clone(), chain_c.clone()).await?;
    spawn(async move {
        if let Err(e) = actor.run(rx).await {
            tracing::error!("expand actor {}|{} exited with error: {:?}", uid_c, chain_c, e);
        }
    });

    map.insert(key, handle.clone());
    Ok(handle)
}

// The actor state and implementation
struct ExpandActor {
    uid: String,
    chain: String,
    // indices that already have an account row (from DB)
    existing_indices: BTreeSet<i32>,
}

impl ExpandActor {
    pub async fn new(
        uid: String,
        chain: String,
    ) -> Result<ExpandActor, crate::error::service::ServiceError> {
        // load existing indices & completed indices from DB
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool().unwrap();

        let existing_accounts: Vec<u32> =
            ApiAccountRepo::get_all_account_indices(&pool, &uid, &chain).await?;
        let existing_indices: BTreeSet<i32> = existing_accounts
            .into_iter()
            .map(|id| {
                // account id -> input_index
                wallet_utils::address::AccountIndexMap::from_account_id(id)
                    .map(|m| m.input_index)
                    .unwrap_or_default()
            })
            .collect();

        Ok(ExpandActor { uid, chain, existing_indices })
    }

    async fn run(mut self, mut rx: mpsc::Receiver<ExpandActorMsg>) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor started");

        while let Some(msg) = rx.recv().await {
            match msg {
                ExpandActorMsg::NewExpandTask { task_id, msg, reply } => {
                    let r = self.handle_new_expand(task_id.clone(), msg).await;
                    if let Some(tx) = reply {
                        let _ = tx.send(r);
                    }
                }
                ExpandActorMsg::AddressInited { indices } => {
                    if let Err(e) = self.handle_address_inited(indices).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle address inited");
                    }
                }
                ExpandActorMsg::RecoverTask { task_id, batch_id, reply } => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "Handling recover task");
                    let r = self.handle_recover_task(&task_id, &batch_id).await;
                    if let Some(tx) = reply {
                        let _ = tx.send(r);
                    }
                }
                ExpandActorMsg::Shutdown => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "shutting down actor");
                    break;
                }
            }
        }

        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor stopped");
        Ok(())
    }

    /// handle incoming expand task
    async fn handle_new_expand(
        &mut self,
        task_id: String,
        msg: AwmCmdAddrExpandMsg,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        tracing::info!(
            "开始处理地址扩容任务: task_id={}, uid={}, chain={}, batch_id={}, number={}, type={:?}, index={:?}, serial_no={}",
            task_id,
            self.uid,
            self.chain,
            msg.batch_id,
            msg.number,
            msg.typ,
            msg.index,
            msg.serial_no
        );

        // compute needed indices using your helper
        let needed: Vec<i32> = AwmCmdAddrExpandMsg::get_needed_indices(
            &msg.typ,
            &self.chain,
            msg.number,
            msg.index,
            &self.uid,
            Some(&task_id),
        )
        .await?;
        tracing::info!(
            "计算所需索引完成: uid={}, chain={}, 索引数量={}, 索引列表={:?}",
            self.uid,
            self.chain,
            needed.len(),
            needed
        );

        ExpandBatchRepo::create_batch(
            &pool,
            &msg.batch_id,
            &msg.serial_no,
            &self.chain,
            needed.len() as i32,
        )
        .await?;
        ExpandBatchItemRepo::batch_create_items(
            &pool,
            &self.uid,
            &msg.batch_id,
            &self.chain,
            &needed,
        )
        .await?;

        self.handle_recover_task(&task_id, &msg.batch_id).await
    }

    async fn handle_recover_task(
        &mut self,
        task_id: &str,
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "开始处理恢复任务");

        let needed: BTreeSet<i32> = ExpandBatchItemRepo::get_items_by_batch_id(&pool, batch_id)
            .await?
            .into_iter()
            .map(|i| i.input_index)
            .collect();

        if needed.is_empty() {
            tracing::info!("recover: needed 为空，跳过");
            return Ok(());
        }

        // 先从数据库加载现有的索引，包括已创建和已初始化的
        self.reload_existing_from_db().await?;

        let completed = self.get_completed_indices_from_db().await?;
        let existing = self.existing_indices.clone();
        tracing::info!(
            "recover reload: needed={:?}, existing={:?}, completed={:?}",
            needed,
            existing,
            completed
        );

        let mut to_create = Vec::new();
        let mut to_init = Vec::new();

        for idx in &needed {
            if !existing.contains(idx) {
                to_create.push(*idx);
            } else if !completed.contains(idx) {
                to_init.push(*idx);
            }
        }

        tracing::info!("recover plan: to_create={:?}, to_init={:?}", to_create, to_init);

        // 如果有需要创建的账户索引
        if !to_create.is_empty() {
            let password = ApiWalletDomain::get_passwd().await?;
            let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
            let wallet: ApiWalletEntity = ApiWalletRepo::find_by_uid(&pool, &self.uid)
                .await?
                .ok_or(ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )))?;

            ApiAccountDomain::create_sub_account(
                &wallet.address,
                &self.uid,
                &password,
                &self.chain,
                "账户",
                true,
                to_create.len() as u32,
                to_create.clone(),
                Some(batch_id.to_string()),
            )
            .await?;

            tracing::info!("recover: 已补创建账户: {:?}", to_create);
        }

        if !to_init.is_empty() {
            let sn = CONTEXT.get().unwrap().get_sn();
            let mut init_req = ApiAddressInitReq::new().with_batch_id(batch_id);

            let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
            let api_wallet = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?.ok_or(
                ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )),
            )?;

            let accounts = ApiAccountRepo::list_by_wallet_address(
                &pool,
                &api_wallet.address,
                None,
                Some(&self.chain),
            )
            .await?;

            for account in accounts {
                if let Ok(map) =
                    wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)
                {
                    let idx = map.input_index;
                    if to_init.contains(&idx) {
                        init_req.address_list.add_address(AddressInitReq::new(
                            &self.uid,
                            &account.address,
                            idx,
                            &self.chain,
                            sn,
                            vec!["".to_string()],
                            &account.name,
                        ));
                    }
                }
            }

            if !init_req.address_list.0.is_empty() {
                let data = BackendApiTaskData::new(
                    wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
                    &init_req,
                )?;
                Tasks::new().push(BackendApiTask::BackendApi(data)).send().await?;
                tracing::info!("recover: 已补发送 init: {:?}", to_init);
            }
        }

        // check if some batch already done
        tracing::debug!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "检查批次完成状态");
        self.check_and_complete_batches().await?;

        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "任务恢复处理完成");
        Ok(())
    }

    async fn handle_address_inited(
        &mut self,
        indices: Vec<i32>, // 修改为接受索引数组
    ) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, indices=?indices, "处理地址初始化完成（批量）");
        if indices.is_empty() {
            return Ok(());
        }

        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1️⃣ 从 DB 反查：这些 indices 命中了哪些 batch + 各自数量
        let affected =
            ExpandBatchItemRepo::find_batches_by_indices(&pool, &self.uid, &self.chain, &indices)
                .await?;

        tracing::info!(
            uid = %self.uid,
            chain = %self.chain,
            affected_batches = ?affected,
            "找到受影响的批次"
        );

        // 2️⃣ 原子增加每个 batch 的 finished 计数
        for (batch_id, count) in &affected {
            if *count > 0 {
                ExpandBatchRepo::increment_finished(&pool, batch_id, *count as usize).await?;
                tracing::info!(
                    uid = %self.uid,
                    chain = %self.chain,
                    batch_id = %batch_id,
                    count = %count,
                    "已更新扩容批次完成计数"
                );
            }
        }
        // 如内存中仍缓存 completed_indices，则从 DB 刷新一次
        self.get_completed_indices_from_db().await?;

        // check serials for completion and trigger callbacks
        self.check_and_complete_batches().await?;

        Ok(())
    }

    async fn reload_existing_from_db(&mut self) -> Result<(), ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let ex: Vec<u32> =
            ApiAccountRepo::get_all_account_indices(&pool, &self.uid, &self.chain).await?;
        self.existing_indices = ex
            .into_iter()
            .map(|id| {
                // account id -> input_index
                wallet_utils::address::AccountIndexMap::from_account_id(id)
                    .map(|m| m.input_index)
                    .unwrap_or_default()
            })
            .collect();
        Ok(())
    }

    async fn check_and_complete_batches(&mut self) -> Result<(), ServiceError> {
        tracing::info!("开始检查和完成地址扩容批次: uid={}, chain={}", self.uid, self.chain);
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let done =
            ExpandBatchRepo::get_done_but_not_notified(&pool, &self.uid, &self.chain).await?;
        for batch in done {
            let backend = CONTEXT.get().unwrap().get_global_backend_api();
            backend
                .expand_address_complete(ExpandAddressCompleteReq::new(
                    &self.uid,
                    &batch.batch_id,
                    &batch.serial_no,
                    true,
                    None,
                ))
                .await?;

            ExpandBatchRepo::mark_as_notified(&pool, &batch.batch_id).await?;
        }

        Ok(())
    }

    // 从数据库获取已完成的索引
    /// 从数据库获取已完成的索引
    async fn get_completed_indices_from_db(&self) -> Result<BTreeSet<i32>, ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        let completed: Vec<(i32,)> =
            ApiAccountRepo::list_inited_indices(&pool, &api_wallet.address, &self.chain)
                .await
                .unwrap_or_default();
        let completed_indices: BTreeSet<i32> =
            completed.into_iter().map(|id: (i32,)| id.0).collect();
        Ok(completed_indices)
    }
}

// ===== Helper APIs for external use =====

/// Submit a new expand task to the actor system
pub async fn submit_expand_task(
    task_id: String,
    msg: AwmCmdAddrExpandMsg,
) -> Result<(), ServiceError> {
    tracing::info!("submit_expand_task -------------- 1");
    let actor: ExpandActorHandle = get_or_create_actor(&msg.uid, &msg.chain_code).await?;
    tracing::info!("submit_expand_task -------------- 2");
    let (tx, rx) = oneshot::channel();
    actor.send(ExpandActorMsg::NewExpandTask { task_id, msg, reply: Some(tx) }).await?;
    rx.await.map_err(|_| {
        ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
            "actor reply dropped".into(),
        ))
    })??;
    Ok(())
}

/// 恢复未完成的expand_address_complete操作
/// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
pub async fn recover_unfinished_expand_complete() -> Result<(), ServiceError> {
    tracing::info!("开始恢复未完成的地址扩展完成操作");

    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

    let done = ExpandBatchRepo::get_all_done_but_not_notified(&pool).await?;

    let mut recovered = 0;

    for batch in done {
        backend
            .expand_address_complete(ExpandAddressCompleteReq::new(
                &batch.uid,
                &batch.batch_id,
                &batch.serial_no,
                true,
                None,
            ))
            .await?;

        ExpandBatchRepo::mark_as_notified(&pool, &batch.batch_id).await?;
        recovered += 1;
    }
    tracing::info!("地址扩展完成恢复结束，共恢复 {} 个批次", recovered);
    Ok(())
}

/// Called from ADDRESS_INIT handler to let actor know an index has been inited
pub async fn submit_address_inited(
    uid: String,
    chain: String,
    indices: Vec<i32>, // 修改为接受索引数组
) -> Result<(), ServiceError> {
    let actor: ExpandActorHandle = get_or_create_actor(&uid, &chain).await?;
    actor.send(ExpandActorMsg::AddressInited { indices }).await?;
    Ok(())
}

pub async fn submit_recover_task(
    task_id: String,
    msg: AwmCmdAddrExpandMsg,
) -> Result<(), ServiceError> {
    tracing::info!(task_id=%task_id, uid=%msg.uid, chain_code=%msg.chain_code, "开始提交恢复任务");
    let actor: ExpandActorHandle = get_or_create_actor(&msg.uid, &msg.chain_code).await?;
    tracing::info!(task_id=%task_id, uid=%msg.uid, chain_code=%msg.chain_code, "Actor已获取");

    let (tx, rx) = oneshot::channel();
    actor
        .send(ExpandActorMsg::RecoverTask {
            task_id: task_id.clone(),
            batch_id: msg.batch_id.clone(),
            reply: Some(tx),
        })
        .await?;
    tracing::info!(task_id=%task_id, "恢复任务已成功发送到Actor");
    rx.await.map_err(|_| {
        ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
            "actor reply dropped".into(),
        ))
    })??;
    Ok(())
}
