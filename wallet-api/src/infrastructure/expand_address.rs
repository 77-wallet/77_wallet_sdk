/// 任何事件
///    ↓
/// ExpandActorMsg::Schedule
///    ↓
/// handle_schedule()
///    ↓
/// 派发 Worker Job
///    ↓
/// Worker 完成 → 再 Schedule
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
    entities::{api_wallet::ApiWalletEntity, expand_batch_item::ExpandItemStatus},
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

const ACTOR_CHANNEL_SIZE: usize = 256;

use tokio::sync::Semaphore;

const EXPAND_MAX_INFLIGHT: usize = 64;

#[derive(Debug)]
enum ExpandJob {
    Create { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
    Init { uid: String, chain: String, batch_id: String, indices: Vec<i32> },
}

struct ExpandWorkerPool {
    sem: Arc<Semaphore>,
    tx: mpsc::Sender<ExpandJob>,
}

static WORKER_POOL: Lazy<ExpandWorkerPool> = Lazy::new(|| {
    let (tx, mut rx) = mpsc::channel::<ExpandJob>(1024);
    let sem = Arc::new(Semaphore::new(EXPAND_MAX_INFLIGHT));

    let sem_c = sem.clone();
    spawn(async move {
        while let Some(job) = rx.recv().await {
            let permit = sem_c.clone().acquire_owned().await.unwrap();
            spawn(async move {
                let _p = permit;
                if let Err(e) = run_expand_job(job).await {
                    tracing::error!("expand worker job failed: {:?}", e);
                }
            });
        }
    });

    ExpandWorkerPool { sem, tx }
});

async fn run_expand_job(job: ExpandJob) -> Result<(), ServiceError> {
    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    let (uid, chain, batch_id, indices) = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            (uid.clone(), chain.clone(), batch_id.clone(), indices.clone())
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            (uid.clone(), chain.clone(), batch_id.clone(), indices.clone())
        }
    };

    let result = match &job {
        ExpandJob::Create { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址创建任务");
            ExpandActor::create_account(&uid, &chain, &indices, &batch_id).await
        }
        ExpandJob::Init { uid, chain, batch_id, indices } => {
            tracing::info!(uid=%uid, chain=%chain, batch_id=%batch_id, "开始执行地址初始化任务");
            ExpandActor::init_account(&uid, &chain, &indices, &batch_id).await
        }
    };

    match result {
        Ok(_) => {
            match job {
                ExpandJob::Create { .. } => {
                    // Create 成功 → Initing
                    ExpandBatchItemRepo::mark_items_status_from(
                        &pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Creating,
                        ExpandItemStatus::Initing,
                    )
                    .await?;
                }
                ExpandJob::Init { .. } => {
                    // Init 成功 → Done
                    ExpandBatchItemRepo::mark_items_status_from(
                        &pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Initing,
                        ExpandItemStatus::Done,
                    )
                    .await?;
                }
            }
        }
        Err(e) => {
            match job {
                ExpandJob::Create { .. } => {
                    // Create 失败 → Failed
                    ExpandBatchItemRepo::mark_items_status_from(
                        &pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Creating,
                        ExpandItemStatus::Failed,
                    )
                    .await?;
                }
                ExpandJob::Init { .. } => {
                    // Init 失败 → Failed
                    ExpandBatchItemRepo::mark_items_status_from(
                        &pool,
                        &batch_id,
                        &indices,
                        ExpandItemStatus::Initing,
                        ExpandItemStatus::Failed,
                    )
                    .await?;
                }
            }
            return Err(e);
        }
    }

    // 跑完后通知 Actor 重新 schedule
    let actor = get_or_create_actor(&uid, &chain).await?;
    actor.send(ExpandActorMsg::Schedule).await?;
    Ok(())
}

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
    RecoverTask { reply: Option<oneshot::Sender<Result<(), ServiceError>>> },
    /// Schedule a check for completed batches
    Schedule,
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
    scheduling: bool,
    schedule_pending: bool,
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

        Ok(ExpandActor { uid, chain, existing_indices, scheduling: false, schedule_pending: false })
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
                ExpandActorMsg::RecoverTask { reply } => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "Handling recover task");
                    // let r = self.handle_recover_task(&task_id, &batch_id).await;
                    let r = self.handle_schedule().await;
                    if let Some(tx) = reply {
                        let _ = tx.send(r);
                    }
                }
                ExpandActorMsg::Schedule => {
                    if !self.schedule_pending {
                        self.schedule_pending = true;
                        if let Err(e) = self.handle_schedule().await {
                            tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle schedule");
                        }
                        self.schedule_pending = false;
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

    async fn handle_schedule(&mut self) -> Result<(), ServiceError> {
        if self.scheduling {
            return Ok(());
        }
        self.scheduling = true;

        let r = self.handle_schedule_inner().await;
        self.scheduling = false;
        r
    }

    async fn handle_schedule_inner(&mut self) -> Result<(), ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1️⃣ 统计 inflight 数量（Creating / Initing）
        let inflight = ExpandBatchItemRepo::count_inflight(&pool, &self.uid, &self.chain).await?;
        let quota = EXPAND_MAX_INFLIGHT.saturating_sub(inflight as usize);

        self.reload_existing_from_db().await?;
        if quota == 0 {
            return Ok(());
        }

        // 2️⃣ 取 Pending items
        let items = ExpandBatchItemRepo::fetch_and_mark_pending(
            &pool,
            &self.uid,
            &self.chain,
            quota as i64,
        )
        .await?;

        if items.is_empty() {
            return Ok(());
        }

        tracing::info!(
            uid = %self.uid,
            chain = %self.chain,
            count = items.len(),
            "expand schedule fetched items"
        );

        // 3️⃣ 先按 batch 分组，再区分 create / init
        let mut grouped: HashMap<String, (Vec<i32>, Vec<i32>)> = HashMap::new();

        for it in items {
            let entry =
                grouped.entry(it.batch_id.clone()).or_insert_with(|| (Vec::new(), Vec::new()));

            if !self.existing_indices.contains(&it.input_index) {
                entry.0.push(it.input_index); // to_create
            } else {
                entry.1.push(it.input_index); // to_init
            }
        }

        for (batch_id, (to_create, to_init)) in grouped {
            // 4️⃣ 标记状态
            if !to_create.is_empty() {
                let res = WORKER_POOL
                    .tx
                    .send(ExpandJob::Create {
                        uid: self.uid.clone(),
                        chain: self.chain.clone(),
                        batch_id: batch_id.clone(),
                        indices: to_create.clone(),
                    })
                    .await;

                if res.is_err() {
                    // 🔁 回滚为 Pending
                    ExpandBatchItemRepo::rollback_status(
                        &pool,
                        &batch_id,
                        &to_create,
                        ExpandItemStatus::Creating,
                        ExpandItemStatus::Pending,
                    )
                    .await?;
                }
            }

            if !to_init.is_empty() {
                ExpandBatchItemRepo::mark_items_status_from(
                    &pool,
                    &batch_id,
                    &to_init,
                    ExpandItemStatus::Creating,
                    ExpandItemStatus::Initing,
                )
                .await?;
                let res = WORKER_POOL
                    .tx
                    .send(ExpandJob::Init {
                        uid: self.uid.clone(),
                        chain: self.chain.clone(),
                        batch_id: batch_id.clone(),
                        indices: to_init.clone(),
                    })
                    .await;
                if res.is_err() {
                    // 🔁 回滚为 Pending
                    ExpandBatchItemRepo::rollback_status(
                        &pool,
                        &batch_id,
                        &to_init,
                        ExpandItemStatus::Initing,
                        ExpandItemStatus::Pending,
                    )
                    .await?;
                }
            }
        }

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

        // self.handle_recover_task(&task_id, &msg.batch_id).await
        self.handle_schedule().await?;
        Ok(())
    }

    // async fn handle_recover_task(
    //     &mut self,
    //     task_id: &str,
    //     batch_id: &str,
    // ) -> Result<(), ServiceError> {
    //     let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
    //     tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "开始处理恢复任务");

    //     let needed: BTreeSet<i32> = ExpandBatchItemRepo::get_items_by_batch_id(&pool, batch_id)
    //         .await?
    //         .into_iter()
    //         .map(|i| i.input_index)
    //         .collect();

    //     if needed.is_empty() {
    //         tracing::info!("recover: needed 为空，跳过");
    //         return Ok(());
    //     }

    //     // 先从数据库加载现有的索引，包括已创建和已初始化的
    //     self.reload_existing_from_db().await?;

    //     let completed = self.get_completed_indices_from_db().await?;
    //     let existing = self.existing_indices.clone();
    //     tracing::info!(
    //         "recover reload: needed={:?}, existing={:?}, completed={:?}",
    //         needed,
    //         existing,
    //         completed
    //     );

    //     let mut to_create = Vec::new();
    //     let mut to_init = Vec::new();

    //     for idx in &needed {
    //         if !existing.contains(idx) {
    //             to_create.push(*idx);
    //         } else if !completed.contains(idx) {
    //             to_init.push(*idx);
    //         }
    //     }

    //     tracing::info!("recover plan: to_create={:?}, to_init={:?}", to_create, to_init);

    //     // 如果有需要创建的账户索引
    //     if !to_create.is_empty() {
    //         Self::create_account(self.uid.as_str(), self.chain.as_str(), to_create, batch_id)
    //             .await?;
    //     }

    //     if !to_init.is_empty() {
    //         Self::init_account(self.uid.as_str(), self.chain.as_str(), to_init, batch_id).await?;
    //     }

    //     // check if some batch already done
    //     tracing::debug!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "检查批次完成状态");
    //     self.check_and_complete_batches().await?;

    //     tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "任务恢复处理完成");
    //     Ok(())
    // }

    async fn create_account(
        uid: &str,
        chain: &str,
        to_create: &[i32],
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let password = ApiWalletDomain::get_passwd().await?;
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet: ApiWalletEntity = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            )),
        )?;

        ApiAccountDomain::create_sub_account(
            &wallet.address,
            uid,
            &password,
            chain,
            "账户",
            true,
            to_create.len() as u32,
            to_create.to_vec(),
            Some(batch_id.to_string()),
        )
        .await?;

        Ok(())
    }

    async fn init_account(
        uid: &str,
        chain: &str,
        to_init: &[i32],
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let sn = CONTEXT.get().unwrap().get_sn();
        let mut init_req = ApiAddressInitReq::new().with_batch_id(batch_id);

        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid).await?.ok_or(
            ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            )),
        )?;

        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, &api_wallet.address, None, Some(chain))
                .await?;

        for account in accounts {
            if let Ok(map) =
                wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)
            {
                let idx = map.input_index;
                if to_init.contains(&idx) {
                    init_req.address_list.add_address(AddressInitReq::new(
                        uid,
                        &account.address,
                        idx,
                        chain,
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

        // check serials for completion and trigger callbacks
        self.check_and_complete_batches().await?;
        self.handle_schedule().await?;
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
    uid: &str,
    chain: &str,
    indices: Vec<i32>, // 修改为接受索引数组
) -> Result<(), ServiceError> {
    let actor: ExpandActorHandle = get_or_create_actor(uid, chain).await?;
    actor.send(ExpandActorMsg::AddressInited { indices }).await?;
    Ok(())
}

// pub async fn submit_recover_task() -> Result<(), ServiceError> {
//     let actor: ExpandActorHandle =
//         get_or_create_actor(&msg.uid, &msg.chain_code, &msg.batch_id).await?;
//     let (tx, rx) = oneshot::channel();
//     actor.send(ExpandActorMsg::RecoverTask { reply: Some(tx) }).await?;
//     rx.await.map_err(|_| {
//         ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
//             "actor reply dropped".into(),
//         ))
//     })??;
//     Ok(())
// }
