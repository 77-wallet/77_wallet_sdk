// expand_actor.rs
// Actor-based expand address manager for your wallet system.
// - Supervisor manages per-(uid,chain) ExpandActor
// - Each ExpandActor runs in a single tokio task and serializes all operations
// - On startup, supervisor can recover unfinished tasks from TaskQueueRepo
// - ADDRESS_INIT events and incoming expand tasks are sent to the actor

use once_cell::sync::Lazy;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};
use tokio::{
    spawn,
    sync::{Mutex, mpsc, oneshot},
};
use wallet_database::{
    entities::api_wallet::ApiWalletEntity,
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, expand_batch::ExpandBatchRepo, wallet::ApiWalletRepo,
        },
        task_queue::TaskQueueRepo,
    },
};

use crate::{
    context::CONTEXT,
    domain::api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    messaging::mqtt::topics::api_wallet::cmd::address_allock::{AwmCmdAddrExpandMsg, ExpandStatus},
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
        task_ids: Vec<String>, // optional list of related tasks to touch
        uid: String,
        chain: String,
        indices: Vec<i32>, // 支持多个索引
    },
    /// Recover existing task (used on startup)
    RecoverTask {
        task_id: String,
        status: ExpandStatus,
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

// /// Initialize address expansion manager
// pub async fn init() -> Result<(), ServiceError> {
//     // Just need to initialize the supervisor - the Lazy static will take care of creating it when needed
//     // We can call the existing init_expand_supervisor function to recover any unfinished tasks
//     init_expand_supervisor().await
// }

// /// Initialize supervisor: recover unfinished expand tasks into actors
// pub async fn init_expand_supervisor() -> Result<(), ServiceError> {
//     let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

//     // find tasks with name AwmCmdAddrExpand in states pending/inprogress/failed
//     let tasks = TaskQueueRepo::list_tasks_with_task_name(
//         &pool,
//         TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
//         &[0, 1, 2, 3],
//     )
//     .await?;

//     for task in tasks {
//         if let Some(ref remark) = task.remark {
//             // try load remark, fallback to constructing from request body
//             match ExpandStatus::load_or_fix_remark(&task).await {
//                 Ok(mut status) => {
//                     let chain = status.chain_code.clone();
//                     let actor = get_or_create_actor(&status.uid, &chain).await?;
//                     let _ = actor
//                         .send(ExpandActorMsg::RecoverTask { task_id: task.id.clone(), status })
//                         .await;
//                 }
//                 Err(err) => {
//                     tracing::warn!("failed to load remark for recover task {}: {:?}", task.id, err)
//                 }
//             }
//         }
//     }

//     Ok(())
// }

// 批次信息结构体，确保batch和serial_no一一对应
#[derive(Debug, Clone)]
struct BatchInfo {
    // 该批次包含的索引集合
    indices: HashSet<i32>,
    // 该批次对应的serial_no
    serial_no: String,
}

// 已完成批次信息，包含批次ID和对应的serial_no
struct CompletedBatchInfo {
    // 批次ID
    batch_id: String,
    // 批次对应的serial_no
    serial_no: String,
}

// The actor state and implementation
struct ExpandActor {
    uid: String,
    chain: String,
    // indices that already have an account row (from DB)
    existing_indices: BTreeSet<i32>,
    // indices that are waiting to be created/initialized
    needed_indices: BTreeSet<i32>,
    // indices that have been created but not yet initialized
    created_indices: BTreeSet<i32>,
    // indices that have been initialized (init reported)
    completed_indices: BTreeSet<i32>,
    // 批次信息映射，batch_id -> BatchInfo，确保indices和serial_no一一对应
    batch_info: HashMap<String, BatchInfo>,
    // mapping task_id -> batch_id (so we can update TaskQueue remark)
    task_to_batch: HashMap<String, String>,
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
        // 初始化为空集合，将在恢复任务或创建账户时填充
        let created_indices: BTreeSet<i32> = BTreeSet::new();

        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        let completed: Vec<(i32,)> =
            ApiAccountRepo::list_inited_indices(&pool, &api_wallet.address, &chain)
                .await
                .unwrap_or_default();
        let completed_indices: BTreeSet<i32> = completed.into_iter().map(|id| id.0).collect();

        Ok(ExpandActor {
            uid,
            chain,
            existing_indices,
            // 初始化为空集合，只有在收到扩容任务时才会添加需要的索引
            needed_indices: BTreeSet::new(),
            created_indices,
            completed_indices,
            batch_info: HashMap::new(),
            task_to_batch: HashMap::new(),
        })
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
                ExpandActorMsg::AddressInited { task_ids, uid: _, chain: _, indices } => {
                    if let Err(e) = self.handle_address_inited(task_ids, indices).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle address inited");
                    }
                }
                ExpandActorMsg::RecoverTask { task_id, status, reply } => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "Handling recover task");
                    let r = self.handle_recover_task(&task_id, status).await;
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

        // 存储batch_info，确保indices和serial_no一一对应

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
        // filter out existing or already completed
        tracing::info!("已存在索引数: {:?}", self.existing_indices);
        let existing_count = needed.iter().filter(|i| self.existing_indices.contains(i)).count();
        tracing::info!("已完成索引数: {:?}", self.completed_indices);
        let completed_indices: BTreeSet<i32> =
            needed.iter().filter(|i| self.completed_indices.contains(i)).cloned().collect();
        let completed_count = completed_indices.len();

        let new_needed: Vec<i32> = needed
            .iter()
            .filter(|i| !self.existing_indices.contains(i))
            .filter(|i| !self.completed_indices.contains(i))
            .cloned()
            .collect();

        tracing::info!(
            "过滤已有和已完成索引: uid={}, chain={}, 原索引数={}, 已存在索引数={}, 已完成索引数={}, 新增索引数={}, 新增索引={:?}",
            self.uid,
            self.chain,
            needed.len(),
            existing_count,
            completed_count,
            new_needed.len(),
            new_needed
        );
        if new_needed.is_empty() {
            tracing::info!(
                "无需创建新账户: uid={}, chain={}, batch_id={}, 所有所需索引已存在或已完成",
                self.uid,
                self.chain,
                msg.batch_id
            );

            // nothing to do: mark task remark/status accordingly
            // load or build remark and set status true
            // 使用new_needed而不是needed，确保remark中的索引与实际要创建的索引一致
            let mut remark = ExpandStatus::new(
                &self.uid,
                &self.chain,
                &new_needed,
                self.completed_indices.clone(),
                false,
                new_needed.len() as u32,
                &msg.serial_no,
                &msg.batch_id,
            );
            remark.addresses_completed =
                remark.needed_indices.iter().all(|i| remark.completed_indices.contains(i));

            tracing::info!(
                "更新任务状态: task_id={}, uid={}, status={}, 已完成索引数={}/{}，batch_id={}",
                task_id,
                self.uid,
                remark.addresses_completed,
                remark.completed_indices.len(),
                remark.needed_indices.len(),
                msg.batch_id
            );

            let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
            TaskQueueRepo::update_task_remark(
                &CONTEXT.get().unwrap().get_global_sqlite_pool()?,
                &task_id,
                &updated,
            )
            .await?;
            tracing::debug!("任务备注更新完成: task_id={}, uid={}", task_id, self.uid);
            // 不再在单个任务完成时调用expand_address_complete
            // 而是在check_and_complete_batches方法中统一检查批次完成状态后调用一次
            if remark.addresses_completed {
                tracing::info!(
                    "单个任务完成: uid={}, batch_id={}, 状态=成功",
                    self.uid,
                    msg.batch_id
                );
            }
            return Ok(());
        }

        // register batch_info with serial_no and indices
        let batch_info = self.batch_info.entry(msg.batch_id.clone()).or_insert_with(|| BatchInfo {
            indices: HashSet::new(),
            serial_no: msg.serial_no.clone(),
        });
        batch_info.indices.extend(new_needed.iter().copied());
        self.task_to_batch.insert(task_id.clone(), msg.batch_id.clone());

        tracing::debug!(
            "更新批处理映射关系: uid={}, batch_id={}, 任务数={}, 批处理映射总数={}",
            self.uid,
            msg.batch_id,
            new_needed.len(),
            batch_info.indices.len()
        );

        // persist remark for this task (so recovery can rebuild if process dies)
        // 使用new_needed而不是needed，确保remark中的索引与实际要创建的索引一致
        let remark = ExpandStatus::new(
            &self.uid,
            &self.chain,
            &new_needed,
            // self.completed_indices.clone(),
            completed_indices,
            false,
            new_needed.len() as u32,
            &msg.serial_no,
            &msg.batch_id,
        );

        tracing::debug!(
            "准备持久化任务状态: task_id={}, uid={}, needed_indices_count={}, completed_indices_count={}",
            task_id,
            self.uid,
            remark.needed_indices.len(),
            remark.completed_indices.len()
        );

        let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
        TaskQueueRepo::update_task_remark(
            &CONTEXT.get().unwrap().get_global_sqlite_pool()?,
            &task_id,
            &updated,
        )
        .await?;

        tracing::info!("任务状态持久化完成: task_id={}, uid={}", task_id, self.uid);

        // 创建扩容批次记录
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ExpandBatchRepo::create_batch(&pool, &msg.batch_id, &self.chain, new_needed.len() as i32)
            .await?;
        tracing::info!(
            "扩容批次记录创建完成: batch_id={}, uid={}, chain={}, total_count={}",
            msg.batch_id,
            self.uid,
            self.chain,
            new_needed.len()
        );

        // actually create sub accounts (this will insert rows into DB but NOT mark init)
        tracing::info!(
            "开始创建子账户: uid={}, chain={}, 子账户数量={}, batch_id={}",
            self.uid,
            self.chain,
            new_needed.len(),
            msg.batch_id
        );

        let password = ApiWalletDomain::get_passwd().await?;
        let wallet: ApiWalletEntity = ApiWalletRepo::find_by_uid(
            &CONTEXT.get().unwrap().get_global_sqlite_pool()?,
            &self.uid,
        )
        .await?
        .ok_or(crate::error::service::ServiceError::Business(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        ))?;

        tracing::debug!("钱包信息获取成功: uid={}, wallet_address={}", self.uid, wallet.address);
        ApiAccountDomain::create_sub_account(
            &wallet.address,
            &self.uid,
            &password,
            &self.chain,
            "账户",
            true,
            new_needed.len() as u32,
            new_needed.clone(),
            Some(msg.batch_id),
        )
        .await?;

        tracing::info!(
            "子账户创建完成: uid={}, chain={}, 创建数量={}, 索引={:?}",
            self.uid,
            self.chain,
            new_needed.len(),
            new_needed
        );

        // 更新已创建索引集合
        for index in &new_needed {
            self.created_indices.insert(*index);
        }

        // after create_sub_account returns, new rows should exist in DB (but not yet inited)
        // update existing_indices from DB
        tracing::debug!("更新已存在索引缓存: uid={}, chain={}", self.uid, self.chain);
        self.reload_existing_from_db().await?;

        tracing::info!(
            "地址扩容任务处理完成: task_id={}, uid={}, chain={}, 新增账户数={}",
            task_id,
            self.uid,
            self.chain,
            new_needed.len()
        );

        // no immediate init; ADDRESS_INIT will arrive externally and feed AddressInited messages
        Ok(())
    }

    async fn handle_recover_task(
        &mut self,
        task_id: &str,
        status: ExpandStatus,
    ) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "开始处理恢复任务");
        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, needed_indices_count=%status.needed_indices.len(),
                        created_indices_count=%status.created_indices.len(),
                        completed_indices_count=%status.completed_indices.len(),
                        batch_id=%status.batch_id,
                        "恢复任务状态详情");

        // 先从数据库加载现有的索引，包括已创建和已初始化的
        self.reload_existing_from_db().await?;

        // 从数据库获取已完成的索引（已初始化的）
        let db_completed_indices = self.get_completed_indices_from_db().await?;

        // merge remark data into actor state
        // ensure we only keep indices that are relevant
        let needed: HashSet<i32> = status.needed_indices.into_iter().collect();
        let created: HashSet<i32> = status.created_indices.into_iter().collect();
        // 不再从remark中获取completed_indices，只从DB获取

        // 记录合并前的状态
        let needed_before = self.needed_indices.len();
        let created_before = self.created_indices.len();
        let completed_before = self.completed_indices.len();

        // merge - 先合并基础数据
        for i in &needed {
            self.needed_indices.insert(*i);
        }
        for i in &created {
            self.created_indices.insert(*i);
        }

        // 重要：从数据库获取最新的已完成索引，并确保completed_indices只增不减
        // 使用reload_completed_from_db方法合并数据库中的已完成索引
        self.reload_completed_from_db().await?;

        // 确保needed和created集合的正确性：移除已完成的索引
        for i in &self.completed_indices {
            self.needed_indices.remove(i);
            self.created_indices.remove(i);
        }

        // 检查数据库中已存在但未在created或completed中的索引
        // 这些可能是在任务中断期间创建但未记录的索引
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        let all_accounts = ApiAccountRepo::list_by_wallet_address(
            &pool,
            &api_wallet.address,
            None,              // account_id is optional
            Some(&self.chain), // chain_code is optional
        )
        .await?;

        let sn = CONTEXT.get().unwrap().get_sn();

        let mut api_address_init_reqs = Vec::new();

        let init_tasks = TaskQueueRepo::get_tasks_with_request_body_and_task_name(
            &pool,
            wallet_database::entities::task_queue::TaskName::Known(
                wallet_database::entities::task_queue::KnownTaskName::BackendApi,
            ),
            "awallet/aw/address/init",
            &[0, 1, 3, 4],
        )
        .await?;

        for task in init_tasks {
            let request_body: BackendApiTaskData =
                wallet_utils::serde_func::serde_from_str(&task.request_body)?;
            let init_body: ApiAddressInitReq =
                wallet_utils::serde_func::serde_from_value(request_body.body)?;
            api_address_init_reqs.push(init_body);
        }

        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "从数据库中获取到的初始化任务={:?}", api_address_init_reqs);
        let mut init_req = ApiAddressInitReq::new().with_batch_id(&status.batch_id);
        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "开始处理恢复任务，所有账户={:?}", all_accounts);
        for account in all_accounts {
            if let Ok(account_index_map) =
                wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)
            {
                let input_index = account_index_map.input_index;

                // 如果索引在needed_indices中，但尚未标记为created或completed
                if self.needed_indices.contains(&input_index) {
                    if self.created_indices.contains(&input_index)
                        && !self.completed_indices.contains(&input_index)
                    {
                        // 检查该账户是否已初始化
                        if account.is_init == 1 {
                            // 已初始化，标记为completed
                            self.completed_indices.insert(input_index);
                            self.needed_indices.remove(&input_index);
                            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, index=%input_index, "恢复任务时发现已初始化的账户，标记为已完成");
                        } else {
                            // 查询本地是否已有该账户的初始化请求
                            // 如果没有则创建
                            if api_address_init_reqs.iter().any(|t| t.has_account(input_index)) {
                                tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, index=%input_index, "恢复任务时发现已存在的初始化请求，跳过");
                                continue;
                            } else {
                                // 创建初始化请求
                                tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, index=%input_index, "恢复任务时发现未初始化的账户，创建初始化请求");
                                init_req.address_list.add_address(AddressInitReq::new(
                                    &self.uid,
                                    &account.address,
                                    input_index,
                                    &self.chain,
                                    sn,
                                    vec!["".to_string()],
                                    &account.name,
                                ));
                            }

                            // 未初始化，标记为created
                            self.created_indices.insert(input_index);
                            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, index=%input_index, "恢复任务时发现已创建但未初始化的账户，标记为已创建");
                        }
                    }
                }
            }
        }

        // 只有当有地址需要初始化时才发送请求
        if !init_req.address_list.0.is_empty() {
            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "发送地址初始化任务，索引={:?}", init_req.address_list.0);
            let api_address_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
                &init_req,
            )?;

            Tasks::new()
                .push(BackendApiTask::BackendApi(api_address_init_task_data))
                .send()
                .await?;

            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "已发送地址初始化任务");
        }

        // 记录合并后的状态
        tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id,
                        needed_indices_added=%(self.needed_indices.len() - needed_before),
                        created_indices_added=%(self.created_indices.len() - created_before),
                        completed_indices_added=%(self.completed_indices.len() - completed_before),
                        "已合并索引状态");

        // record batch mapping
        let batch_size_before =
            self.batch_info.get(&status.batch_id).map_or(0, |info| info.indices.len());
        // 假设这里已经有serial_no，如果没有，可能需要从某个地方获取
        let serial_no = status.serial_no.clone();
        self.batch_info
            .entry(status.batch_id.clone())
            .or_insert_with(|| BatchInfo { indices: HashSet::new(), serial_no: serial_no.clone() })
            .indices
            .extend(needed.iter().copied());
        self.task_to_batch.insert(task_id.to_string(), status.batch_id.clone());

        tracing::info!(
            "记录批处理映射: uid={}, task_id={}, batch_id={}, 批处理索引增加数量={}, 任务映射总数={}",
            self.uid,
            task_id,
            status.batch_id,
            self.batch_info.get(&status.batch_id).map_or(0, |info| info.indices.len())
                - batch_size_before,
            self.task_to_batch.len()
        );

        // also ensure existing_indices updated
        tracing::debug!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "更新已存在索引");
        self.reload_existing_from_db().await?;

        // 检查哪些需要的索引尚未创建账户
        let mut indices_to_create: Vec<i32> = Vec::new();
        for index in &self.needed_indices {
            // 检查索引是否在已创建或已完成集合中
            if !self.created_indices.contains(index) && !self.completed_indices.contains(index) {
                // 检查数据库中是否存在该索引的账户
                if !self.existing_indices.contains(index) {
                    indices_to_create.push(*index);
                }
            }
        }

        // 如果有需要创建的账户索引
        if !indices_to_create.is_empty() {
            tracing::info!(
                "检测到需要创建的账户: uid={}, chain={}, task_id={}, 需要创建的索引数量={:?}, 索引列表={:?}",
                self.uid,
                self.chain,
                task_id,
                indices_to_create.len(),
                indices_to_create
            );

            // 获取密码
            let password = ApiWalletDomain::get_passwd().await?;
            // 获取钱包信息
            let wallet: ApiWalletEntity = ApiWalletRepo::find_by_uid(
                &CONTEXT.get().unwrap().get_global_sqlite_pool()?,
                &self.uid,
            )
            .await?
            .ok_or(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                ),
            ))?;

            tracing::debug!(
                "钱包信息获取成功: uid={}, wallet_address={}",
                self.uid,
                wallet.address
            );

            // 创建子账户
            ApiAccountDomain::create_sub_account(
                &wallet.address,
                &self.uid,
                &password,
                &self.chain,
                "账户",
                true,
                indices_to_create.len() as u32,
                indices_to_create.clone(),
                Some(status.batch_id.clone()),
            )
            .await?;

            tracing::info!(
                "子账户创建完成: uid={}, chain={}, 创建数量={}, 索引={:?}",
                self.uid,
                self.chain,
                indices_to_create.len(),
                indices_to_create
            );

            // 更新created_indices集合
            for index in &indices_to_create {
                self.created_indices.insert(*index);
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
        task_ids: Vec<String>,
        indices: Vec<i32>, // 修改为接受索引数组
    ) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, indices=?indices, "处理地址初始化完成（批量）");

        // 不再手动更新completed_indices，改为从数据库刷新

        // find affected batches and count how many indices in each batch were completed
        let mut affected_batches = std::collections::HashMap::<String, usize>::new();
        for (batch_id, batch_info) in &self.batch_info {
            // 计算该批次中此次完成的索引数量
            let completed_in_batch =
                indices.iter().filter(|index| batch_info.indices.contains(index)).count();

            if completed_in_batch > 0 {
                affected_batches.insert(batch_id.clone(), completed_in_batch);
            }
        }

        tracing::info!(uid=%self.uid, chain=%self.chain, indices=?indices, affected_batches_count=%affected_batches.len(), "找到受影响的批次");

        // 原子更新每个受影响批次的已完成计数（一次性增加指定数量）
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        for (batch_id, count) in &affected_batches {
            // 为当前批次一次性增加实际完成的索引数量
            ExpandBatchRepo::increment_finished(&pool, batch_id, *count).await?;
            tracing::info!(uid=%self.uid, chain=%self.chain, batch_id=%batch_id, count=%count, "已更新扩容批次完成计数");
        }

        // 从数据库刷新已完成索引到内存中
        self.reload_completed_from_db().await?;
        // 从数据库获取最新的已完成索引用于后续处理
        let completed_indices = self.get_completed_indices_from_db().await?;

        // 立即更新相关任务的备注
        for task_id in &task_ids {
            if let Some(batch_id) = self.task_to_batch.get(task_id) {
                if affected_batches.contains_key(batch_id) {
                    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                    if let Some(task) = TaskQueueRepo::task_detail(&pool, task_id).await? {
                        let mut remark = ExpandStatus::load_or_fix_remark(&task).await?;

                        // 由于needed_indices现在是稳定不变的，我们可以直接计算任务需要的索引中已完成的部分
                        // 从数据库获取所有已完成的索引（input_index格式）
                        let db_existing = self.get_completed_indices_from_db().await?;

                        // 计算任务需要的索引中已完成的部分
                        remark.completed_indices =
                            remark.needed_indices.intersection(&db_existing).copied().collect();

                        // 已创建但未初始化的索引：任务需要的所有索引中已创建但未完成的部分
                        remark.created_indices = remark
                            .needed_indices
                            .intersection(&self.created_indices)
                            .filter(|i| !remark.completed_indices.contains(i))
                            .copied()
                            .collect();

                        // 更新任务状态
                        remark.addresses_completed =
                            remark.needed_indices.iter().all(|i| completed_indices.contains(i));

                        tracing::info!(
                            "更新任务备注: task_id={}, uid={}, chain={}, batch_id={}, indices={:?}, completed_indices={:?}, created_indices={:?}, status={}",
                            task_id,
                            self.uid,
                            self.chain,
                            batch_id,
                            indices,
                            remark.completed_indices,
                            remark.created_indices,
                            remark.addresses_completed
                        );

                        let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
                        TaskQueueRepo::update_task_remark(&pool, task_id, &updated).await?;
                    }
                }
            }
        }

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
        tracing::debug!("成功获取数据库连接池");

        // 从task_to_batch中获取所有关联的batch_id
        let relevant_batches: HashSet<String> = self.task_to_batch.values().cloned().collect();

        tracing::info!(
            "获取相关批次ID: uid={}, chain={}, 批次数量={}",
            self.uid,
            self.chain,
            relevant_batches.len()
        );

        // 从数据库获取最新的已完成索引
        let completed_indices = self.get_completed_indices_from_db().await?;

        // 检查每个批次是否已完成
        let mut completed_batches: Vec<CompletedBatchInfo> = Vec::new();
        for batch_id in relevant_batches {
            // 使用新的expand_batch表检查批次是否已完成
            if let Some(batch_info) = self.batch_info.get(&batch_id) {
                let is_done = ExpandBatchRepo::is_batch_done(&pool, &batch_id).await?;

                tracing::debug!(
                    "检查批次完成状态: batch_id={}, uid={}, chain={}, 是否已完成={}",
                    batch_id,
                    self.uid,
                    self.chain,
                    is_done
                );

                if is_done {
                    // 批次已完成
                    tracing::info!(
                        "批次所有索引已完成: batch_id={}, uid={}, chain={}, serial_no={}",
                        batch_id,
                        self.uid,
                        self.chain,
                        batch_info.serial_no
                    );
                    completed_batches.push(CompletedBatchInfo {
                        batch_id: batch_id.clone(),
                        serial_no: batch_info.serial_no.clone(),
                    });
                }
            } else {
                tracing::warn!(
                    "批次在batch_info中未找到: batch_id={}, uid={}, chain={}",
                    batch_id,
                    self.uid,
                    self.chain
                );
            }
        }

        tracing::info!(
            "批次检查完成: uid={}, chain={}, 已完成批次数量={}",
            self.uid,
            self.chain,
            completed_batches.len()
        );

        for completed_batch in completed_batches {
            let batch_id = &completed_batch.batch_id;
            let serial_no = &completed_batch.serial_no;

            tracing::info!(
                "开始处理已完成批次: batch_id={}, serial_no={}, uid={}, chain={}",
                batch_id,
                serial_no,
                self.uid,
                self.chain
            );

            // remove mapping
            if let Some(batch_info) = self.batch_info.remove(batch_id) {
                tracing::debug!(
                    "从batch_info移除批次: batch_id={}, uid={}, chain={}, 索引数量={}",
                    batch_id,
                    self.uid,
                    self.chain,
                    batch_info.indices.len()
                );

                // find associated task_ids
                let task_ids: Vec<String> = self
                    .task_to_batch
                    .iter()
                    .filter_map(|(tid, s)| if s == batch_id { Some(tid.clone()) } else { None })
                    .collect();
                tracing::info!(
                    "找到批次关联任务: batch_id={}, uid={}, chain={}, 任务数量={}, 任务列表={:?}",
                    batch_id,
                    self.uid,
                    self.chain,
                    task_ids.len(),
                    task_ids
                );

                // 从数据库刷新已完成索引到内存中
                self.reload_completed_from_db().await?;
                tracing::debug!(
                    "更新内存中已完成索引: uid={}, chain={}, 索引数量={}",
                    self.uid,
                    self.chain,
                    self.completed_indices.len()
                );

                // update tasks remarks & states
                for task_id in &task_ids {
                    tracing::info!(
                        "更新任务状态和备注: task_id={}, batch_id={}, uid={}, chain={}",
                        task_id,
                        batch_id,
                        self.uid,
                        self.chain
                    );

                    // 直接查询任务详情，不再使用缓存
                    if let Some(task) = TaskQueueRepo::task_detail(&pool, task_id).await? {
                        tracing::debug!(
                            "成功获取任务详情: task_id={}, uid={}, chain={}, 任务状态={}",
                            task_id,
                            self.uid,
                            self.chain,
                            task.status
                        );

                        let mut remark = ExpandStatus::load_or_fix_remark(&task).await?;
                        // 从数据库获取所有已完成的索引（input_index格式）
                        let db_existing = self.get_completed_indices_from_db().await?;

                        // 计算任务需要的索引中已完成的部分
                        remark.completed_indices =
                            remark.needed_indices.intersection(&db_existing).copied().collect();
                        // 更新已创建但未初始化的索引
                        remark.created_indices =
                            self.created_indices.clone().iter().map(|&i| i).collect();

                        tracing::info!(
                            "加载并更新任务备注: task_id={}, uid={}, chain={}, 任务原始状态={}",
                            task_id,
                            self.uid,
                            self.chain,
                            remark.addresses_completed
                        );

                        // 重要：只在所有needed_indices都完成时才设置status为true
                        remark.addresses_completed =
                            remark.needed_indices.iter().all(|i| completed_indices.contains(i));

                        // 记录当前任务的索引完成情况
                        let completed_count = remark
                            .needed_indices
                            .iter()
                            .filter(|i| completed_indices.contains(i))
                            .count();
                        let created_count = remark
                            .needed_indices
                            .iter()
                            .filter(|i| remark.created_indices.contains(i))
                            .count();
                        let needed_count = remark.needed_indices.len();

                        tracing::info!(
                            "任务索引完成情况: task_id={}, uid={}, chain={}, batch_id={}, needed_indices={:?}, created_indices={:?}, completed_indices={:?}, status={}, 完成进度={}/{}, 创建进度={}/{}  ",
                            task_id,
                            self.uid,
                            self.chain,
                            batch_id,
                            remark.needed_indices,
                            remark.created_indices,
                            remark.completed_indices,
                            remark.addresses_completed,
                            completed_count,
                            needed_count,
                            created_count,
                            needed_count
                        );

                        let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
                        tracing::debug!(
                            "准备更新任务备注: task_id={}, uid={}, chain={}, 更新后的状态={}",
                            task_id,
                            self.uid,
                            self.chain,
                            remark.addresses_completed
                        );

                        TaskQueueRepo::update_task_remark(
                            &CONTEXT.get().unwrap().get_global_sqlite_pool()?,
                            task_id,
                            &updated,
                        )
                        .await?;
                        tracing::info!(
                            "成功更新任务备注: task_id={}, uid={}, chain={}, 任务状态={}",
                            task_id,
                            self.uid,
                            self.chain,
                            remark.addresses_completed
                        );
                    } else {
                        tracing::warn!(
                            "任务不存在于数据库: task_id={}, uid={}, chain={}",
                            task_id,
                            self.uid,
                            self.chain
                        );
                        continue;
                    }
                }

                // 验证所有关联任务的needed_indices是否都已完成
                // 由于我们已经使用expand_batch表确认了批次已完成，这里可以简化验证
                let mut all_tasks_complete = true;
                for task_id in &task_ids {
                    // 直接查询任务详情
                    if let Some(task) = TaskQueueRepo::task_detail(&pool, task_id).await? {
                        if let Ok(remark) = ExpandStatus::load_or_fix_remark(&task).await {
                            // 检查任务是否已标记为完成
                            if !remark.addresses_completed {
                                // 更新任务备注为已完成
                                let mut remark = remark;
                                remark.addresses_completed = true;
                                let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
                                TaskQueueRepo::update_task_remark(&pool, task_id, &updated).await?;
                                tracing::info!("已更新任务状态为完成: task_id={}", task_id);
                            }
                        } else {
                            tracing::error!("加载任务备注失败: task_id={}", task_id);
                        }
                    } else {
                        tracing::error!("任务不存在于数据库: task_id={}", task_id);
                    }
                }

                // 只有当所有任务的needed_indices都已完成时，才调用后端接口
                if all_tasks_complete {
                    let mut all_notified = true;
                    let mut need_notify = false;
                    let mut tasks_to_update = Vec::new();

                    // 单次遍历：检查是否需要通知并收集需要更新的任务
                    for task_id in &task_ids {
                        // 直接查询任务详情，不再使用缓存
                        if let Some(task) = TaskQueueRepo::task_detail(&pool, task_id).await? {
                            if let Some(remark_str) = &task.remark {
                                if let Ok(mut remark) = wallet_utils::serde_func::serde_from_str::<
                                    ExpandStatus,
                                >(remark_str)
                                {
                                    if !remark.notified_complete {
                                        all_notified = false;
                                        need_notify = true;
                                        // 提前准备好需要更新的任务信息
                                        remark.notified_complete = true;
                                        tasks_to_update.push((task_id.clone(), remark));
                                    }
                                }
                            }
                        }
                    }

                    // 如果需要通知，发送一次通知
                    if need_notify {
                        tracing::info!(
                            "所有任务的needed_indices都已完成，准备通知后端批次完成: batch_id={}, uid={}, chain={}",
                            batch_id,
                            self.uid,
                            self.chain
                        );

                        let backend = CONTEXT.get().unwrap().get_global_backend_api();
                        // 直接使用已存储的serial_no
                        let req = ExpandAddressCompleteReq::new(
                            &self.uid, batch_id, serial_no, true, None,
                        );
                        backend.expand_address_complete(req).await?;

                        tracing::info!(
                            "地址扩容完成通知发送成功: batch_id={}, uid={}, chain={}",
                            batch_id,
                            self.uid,
                            self.chain
                        );

                        // 使用预先准备好的任务信息进行更新
                        for (task_id, updated_remark) in tasks_to_update {
                            let remark_str =
                                wallet_utils::serde_func::serde_to_string(&updated_remark)?;
                            TaskQueueRepo::update_task_remark(&pool, &task_id, &remark_str).await?;

                            tracing::info!(
                                "更新任务通知状态: batch_id={}, task_id={}",
                                batch_id,
                                task_id
                            );
                        }
                    }

                    if all_notified {
                        tracing::debug!(
                            "批次已完成但已通知过，跳过重复通知: batch_id={}, uid={}, chain={}",
                            batch_id,
                            self.uid,
                            self.chain
                        );
                    }
                } else {
                    tracing::warn!(
                        "部分任务的needed_indices未全部完成，暂不通知后端: batch_id={}, uid={}, chain={}",
                        batch_id,
                        self.uid,
                        self.chain
                    );
                }

                // 通知日志
                tracing::info!(
                    "地址扩容批次完成通知成功: batch_id={}, uid={}, chain={}, 完成索引数量={}",
                    batch_id,
                    self.uid,
                    self.chain,
                    batch_info.indices.len()
                );

                // remove task_to_serial entries for completed serial
                self.task_to_batch.retain(|_, s| s != batch_id);
            }
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

    /// 从数据库刷新已完成的索引到内存中
    async fn reload_completed_from_db(&mut self) -> Result<(), ServiceError> {
        let completed_indices = self.get_completed_indices_from_db().await?;
        // 只增不减，合并新的已完成索引
        self.completed_indices =
            self.completed_indices.union(&completed_indices).copied().collect();
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

    // 查询所有AwmCmdAddrExpand类型的任务（状态为0,1,2,3）
    let tasks = TaskQueueRepo::list_tasks_with_task_name(
        &pool,
        wallet_database::entities::task_queue::TaskName::Known(
            wallet_database::entities::task_queue::KnownTaskName::AwmCmdAddrExpand,
        ),
        &[],
    )
    .await?;

    tracing::info!("获取到AwmCmdAddrExpand任务数量: {}", tasks.len());

    let mut recovered_count = 0;

    for task in tasks {
        match ExpandStatus::load_or_fix_remark(&task).await {
            Ok(mut remark) => {
                // 检查是否所有需要的索引都已完成，并且还未通知完成
                if !remark.needed_indices.is_empty()
                    && remark.completed_indices.is_superset(&remark.needed_indices)
                    && !remark.notified_complete
                {
                    tracing::info!(
                        "发现需要恢复的任务: task_id={}, uid={}, batch_id={}, 
                        needed_indices={:?}, created_indices_size={}, completed_indices_size={}",
                        task.id,
                        remark.uid,
                        remark.batch_id,
                        remark.needed_indices,
                        remark.created_indices.len(),
                        remark.completed_indices.len()
                    );

                    // 调用expand_address_complete
                    let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
                    let req = ExpandAddressCompleteReq::new(
                        &remark.uid,
                        &remark.batch_id,
                        &remark.serial_no,
                        true,
                        Some(&serde_json::to_string(&remark).map_err(|e| {
                            ServiceError::System(crate::error::system::SystemError::Internal(
                                format!("Serialization error: {}", e),
                            ))
                        })?),
                    );

                    if let Err(e) = backend.expand_address_complete(req).await {
                        tracing::error!(
                            "调用expand_address_complete失败: task_id={}, error={}",
                            task.id,
                            e
                        );
                    } else {
                        // 更新任务备注，标记为已通知完成
                        remark.notified_complete = true;
                        let remark_json = serde_json::to_string(&remark).map_err(|e| {
                            ServiceError::System(crate::error::system::SystemError::Internal(
                                format!("Serialization error: {}", e),
                            ))
                        })?;

                        if let Err(e) =
                            TaskQueueRepo::update_task_remark(&pool, &task.id, &remark_json).await
                        {
                            tracing::error!("更新任务备注失败: task_id={}, error={}", task.id, e);
                        } else {
                            tracing::info!("任务恢复成功: task_id={}", task.id);
                            recovered_count += 1;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("加载任务状态失败: task_id={}, error={}", task.id, e);
            }
        }
    }

    tracing::info!("地址扩展完成操作恢复结束，共恢复 {} 个任务", recovered_count);
    Ok(())
}

/// Called from ADDRESS_INIT handler to let actor know an index has been inited
pub async fn submit_address_inited(
    uid: String,
    chain: String,
    indices: Vec<i32>, // 修改为接受索引数组
    related_task_ids: Vec<String>,
) -> Result<(), ServiceError> {
    let actor: ExpandActorHandle = get_or_create_actor(&uid, &chain).await?;
    actor
        .send(ExpandActorMsg::AddressInited { task_ids: related_task_ids, uid, chain, indices })
        .await?;
    Ok(())
}

pub async fn submit_recover_task(
    task_id: String,
    msg: AwmCmdAddrExpandMsg,
) -> Result<(), ServiceError> {
    tracing::info!(task_id=%task_id, uid=%msg.uid, chain_code=%msg.chain_code, "开始提交恢复任务");

    // 加载或修复任务备注
    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
    let task = TaskQueueRepo::task_detail(&pool, &task_id).await?.ok_or(ServiceError::System(
        crate::error::system::SystemError::Internal("Task not found".into()),
    ))?;

    tracing::debug!(task_id=%task_id, task_status=%task.status, "任务详情已获取");

    let status = ExpandStatus::load_or_fix_remark(&task).await?;
    tracing::info!(task_id=%task_id, needed_indices_count=%status.needed_indices.len(),
                  completed_indices_count=%status.completed_indices.len(), "任务状态已加载");

    let actor: ExpandActorHandle = get_or_create_actor(&msg.uid, &msg.chain_code).await?;
    tracing::info!(task_id=%task_id, uid=%msg.uid, chain_code=%msg.chain_code, "Actor已获取");

    let (tx, rx) = oneshot::channel();
    actor
        .send(ExpandActorMsg::RecoverTask { task_id: task_id.clone(), status, reply: Some(tx) })
        .await?;
    tracing::info!(task_id=%task_id, "恢复任务已成功发送到Actor");

    rx.await.map_err(|_| {
        ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
            "actor reply dropped".into(),
        ))
    })??;

    Ok(())
}
