// actor.rs
/// 决定怎么执行扩容
use std::collections::{BTreeSet, HashMap};
use tokio::sync::{mpsc, oneshot};
use wallet_database::{
    entities::{address_query_state::AddressQueryStatus, expand_batch_item::ExpandItemStatus},
    repositories::api_wallet::{
        account::ApiAccountRepo, address_query_state::AddressQueryStateRepo,
        expand_batch::ExpandBatchRepo, expand_batch_item::ExpandBatchItemRepo,
    },
};
use wallet_utils::address::AccountIndexMap;

use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::expand_address::worker::ExpandJob,
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AwmCmdAddrExpandMsg,
};
pub(crate) const EXPAND_MAX_INFLIGHT: usize = 64;

// Messages the actor understands
#[derive(Debug)]
pub enum ExpandActorMsg {
    /// New expand task arrives (task_id optional, msg struct comes from TaskQueue)
    NewExpandTask {
        task_id: String,
        msg: AwmCmdAddrExpandMsg,
        reply: Option<oneshot::Sender<Result<(), ServiceError>>>,
    },
    AccountCreated {
        indices: Vec<i32>,
    },
    /// Address inited from ADDRESS_INIT handler
    AddressInited {
        indices: Vec<i32>, // 支持多个索引
    },
    NotifyAddressExpanded {
        batch_id: String,
    },
    JobFailed {
        phase: ExpandItemStatus, // Creating 或 Initing
        indices: Vec<i32>,
        error: String,
    },
    // /// Recover existing task (used on startup)
    // ///
    // /// RecoverExpandState 管的是「扩容系统内部的一致性」
    // ///
    // /// RecoverExpandState ≠ 扩容请求本身，它只是“把数据库修成一个可以继续跑的状态”
    // /// - 修复异常中断留下的 item / batch 状态
    // /// - 不创建新业务 item
    // /// - 不判断 address_sync
    // /// - 不决定是否扩容
    // /// - 只让系统“回到一个可被 Schedule 推进的状态”
    // ///
    // /// | 阶段                   | 结果                                  |
    // /// | -------------------- | ----------------------------------- |
    // /// | AddressQuery Running | Recover 会修 DB，但 Schedule 不推进        |
    // /// | AddressQuery Done    | Actor 收到 BackendAddressSynced，再统一推进 |
    // /// 不会直接造成“扩容抢跑”
    // RecoverExpandState {
    //     reply: Option<oneshot::Sender<Result<(), ServiceError>>>,
    // },
    /// Schedule a check for completed batches
    ///
    /// Schedule是执行期信号，不是状态修复信号
    Schedule,
    /// Shutdown actor
    Shutdown,
    BackendAddressSyncing,
    BackendAddressSynced,
}

#[derive(Clone)]
pub struct ExpandActorHandle {
    pub(crate) sender: mpsc::Sender<ExpandActorMsg>,
}

impl ExpandActorHandle {
    pub async fn send(&self, msg: ExpandActorMsg) -> Result<(), ServiceError> {
        self.sender.send(msg).await.map_err(|_| {
            ServiceError::System(crate::error::system::SystemError::Internal("actor closed".into()))
        })
    }
}

/// AddressQuery 管的是「是否允许扩容推进」
/// - 当 address_sync = Syncing 时，expand 会被缓存起来，不进入“执行阶段”
/// - 当 address_sync = Done 时，expand 会被推进到“执行阶段”
///
/// 控制 expand 是否可以进入“执行阶段”
#[derive(Debug)]
enum AddressSyncState {
    Syncing,
    Done,
    Unknown,
}

/// ExpandActor = ExpandFlow for (uid, chain)
/// Phases:
/// - AddressSyncing: backend address not ready, expand cached
/// - Expanding: batch/items driving
/// - Finished: all batches done (actor idle)
///
/// DB is the single source of truth.
/// Tasks and notifications are side effects.
#[derive(Debug)]
pub(crate) struct ExpandActor {
    uid: String,
    chain: String,
    // indices that already have an account row (from DB)
    existing_indices: BTreeSet<i32>,
    scheduling: bool,
    schedule_pending: bool,
    self_sender: mpsc::Sender<ExpandActorMsg>,

    address_sync: AddressSyncState,
    pending_expands: HashMap<String, AwmCmdAddrExpandMsg>,
}

impl ExpandActor {
    pub fn new(uid: String, chain: String, tx: mpsc::Sender<ExpandActorMsg>) -> ExpandActor {
        ExpandActor {
            uid,
            chain,
            self_sender: tx,
            existing_indices: BTreeSet::new(),
            scheduling: false,
            schedule_pending: false,
            address_sync: AddressSyncState::Unknown,
            pending_expands: HashMap::new(),
        }
    }

    async fn load_existing_indices(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let existing_accounts =
            ApiAccountRepo::get_all_account_indices(pool.clone(), &self.uid, &self.chain).await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            count=%existing_accounts.len(),
            accounts=?existing_accounts,
            "load existing account indices"
        );

        self.existing_indices = existing_accounts
            .into_iter()
            .map(|id| {
                AccountIndexMap::from_account_id(id).map(|m| m.input_index).unwrap_or_default()
            })
            .collect();

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            existing_indices=?self.existing_indices,
            "loaded existing indices"
        );

        Ok(())
    }

    async fn init_address_sync_state(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        if let Some(state) =
            AddressQueryStateRepo::get_by_uid_and_chain(&pool, &self.uid, &self.chain).await?
        {
            match state.status {
                AddressQueryStatus::Done => {
                    self.address_sync = AddressSyncState::Done;
                    self.compensate_batches_after_address_sync().await?;
                }
                AddressQueryStatus::Running => {
                    self.address_sync = AddressSyncState::Syncing;
                }
                AddressQueryStatus::Failed => {
                    tracing::warn!(
                        uid=%self.uid,
                        chain=%self.chain,
                        "address query failed, treat as syncing"
                    );
                    self.address_sync = AddressSyncState::Done;
                }
            }
        } else {
            // 🔴 关键：None 不等于 Syncing
            self.address_sync = AddressSyncState::Done;
            // 或 Unknown，看你业务；但 Done 更安全
        }

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            state=?self.address_sync,
            "init address sync state"
        );

        Ok(())
    }

    pub(crate) async fn run(
        mut self,
        mut rx: mpsc::Receiver<ExpandActorMsg>,
    ) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor started");
        self.load_existing_indices().await?;
        self.init_address_sync_state().await?;

        if let Err(e) = self.recover_expand_state().await {
            tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to recover expand state");
        }
        let self_tx = self.self_sender.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let _ = self_tx.send(ExpandActorMsg::Schedule).await;
            }
        });

        while let Some(msg) = rx.recv().await {
            match msg {
                ExpandActorMsg::NewExpandTask { task_id, msg, reply } => {
                    let r = match self.address_sync {
                        AddressSyncState::Syncing | AddressSyncState::Unknown => {
                            self.pending_expands.insert(task_id.clone(), msg);
                            tracing::info!(
                                uid=%self.uid, chain=%self.chain, task_id=%task_id,
                                "Backend address not synced yet, expand pending"
                            );
                            Ok(())
                        }
                        AddressSyncState::Done => {
                            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "Address sync done, handle expand");
                            let r = self.handle_new_expand(task_id.clone(), msg).await;
                            tracing::info!(uid=%self.uid, chain=%self.chain, task_id=%task_id, "Address sync done, handle expand result: {:?}", r);
                            r
                        }
                    };

                    if let Some(tx) = reply {
                        let _ = tx.send(r);
                    }
                }
                ExpandActorMsg::AccountCreated { indices } => {
                    if let Err(e) = self.handle_account_created(indices).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle account created");
                    }
                }
                ExpandActorMsg::AddressInited { indices } => {
                    if let Err(e) = self.handle_address_inited(indices).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle address inited");
                    }
                }
                ExpandActorMsg::NotifyAddressExpanded { batch_id } => {
                    if let Err(e) = self.handle_notify_address_expanded(batch_id).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle notify address expanded");
                    }
                }
                ExpandActorMsg::JobFailed { phase, indices, error } => {
                    if let Err(e) = self.handle_job_failed(phase, indices, error).await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle job failed");
                    }
                }
                // ExpandActorMsg::RecoverExpandState { reply } => {
                //     tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: reset unfinished items");
                //     let r = self.recover_expand_state().await;
                //     tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: reset unfinished items result: {:?}", r);
                //     if let Some(tx) = reply {
                //         let _ = tx.send(r);
                //     }
                // }
                ExpandActorMsg::Schedule => {
                    if !matches!(self.address_sync, AddressSyncState::Done) {
                        tracing::debug!(
                            uid=%self.uid,
                            chain=%self.chain,
                            state=?self.address_sync,
                            "Schedule ignored: address not ready"
                        );
                        continue;
                    }

                    tracing::info!(uid=%self.uid, chain=%self.chain, "Schedule: start");
                    if self.scheduling {
                        self.schedule_pending = true;
                    } else {
                        self.scheduling = true;
                        loop {
                            self.schedule_pending = false;
                            if let Err(e) = self.handle_schedule().await {
                                tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "Failed to handle schedule");
                            }
                            if !self.schedule_pending {
                                tracing::info!(uid=%self.uid, chain=%self.chain, "Schedule: done");
                                break;
                            }
                        }
                        self.scheduling = false;
                    }
                }
                ExpandActorMsg::Shutdown => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "shutting down actor");
                    break;
                }
                ExpandActorMsg::BackendAddressSyncing => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "backend address syncing");
                    self.address_sync = AddressSyncState::Syncing;
                }
                ExpandActorMsg::BackendAddressSynced => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "backend address sync done");
                    if matches!(self.address_sync, AddressSyncState::Done) {
                        tracing::warn!(uid=%self.uid, chain=%self.chain, "BackendAddressSynced called twice, ignore");
                        continue;
                    }
                    self.address_sync = AddressSyncState::Done;
                    // self.compensate_batches_after_address_sync().await?;

                    // 保险起见，reload 一次
                    if let Err(e) = self.reload_existing_from_db().await {
                        tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "reload existing failed");
                    }

                    // 把恢复期缓存的 expand 真正执行
                    let pendings = std::mem::take(&mut self.pending_expands);
                    tracing::info!(uid=%self.uid, chain=%self.chain, count=%pendings.len(), "recover pending expands");

                    for (task_id, msg) in pendings {
                        if let Err(e) = self.handle_new_expand(task_id, msg).await {
                            tracing::error!(uid=%self.uid, chain=%self.chain, error=%e, "handle pending expand failed");
                        }
                    }

                    // 推一次调度
                    let _ = self.self_sender.send(ExpandActorMsg::Schedule).await;
                }
            }
        }

        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor stopped");
        Ok(())
    }

    /// AddressSync 完成后的业务一致性补偿
    async fn compensate_batches_after_address_sync(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let list = ExpandBatchRepo::get_running_batches_with_insufficient_items(
            pool.clone(),
            &self.uid,
            &self.chain,
        )
        .await?;
        tracing::info!("发现 items 数量不足的扩容 batch list={:?}", list);
        if !list.is_empty() {
            tracing::warn!(
                uid=%self.uid,
                chain=%self.chain,
                count=%list.len(),
                "发现 items 数量不足的扩容 batch，开始补建"
            );
        }

        for b in list {
            let need = (b.batch.total_count as i64 - b.item_count) as u32;

            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                batch_id=%b.batch.batch_id,
                total=%b.batch.total_count,
                exist=%b.item_count,
                need=%need,
                "补建 batch items"
            );

            self.handle_recover_expand_items(&b.batch.batch_id, need).await?;
        }

        Ok(())
    }

    /// 修复异常中断留下的 item / batch 状态
    /// - 不创建新业务 item
    /// - 不判断 address_sync
    /// - 不决定是否扩容
    /// - 只让系统“回到一个可被 Schedule 推进的状态”
    /// - notify: Done but not notified → 再补发
    async fn recover_expand_state(&mut self) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: start");
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        // 1️⃣ Failed / Creating / Initing → Pending
        tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: reset unfinished to pending");
        let affected =
            ExpandBatchItemRepo::reset_unfinished_to_pending(pool.clone(), &self.uid, &self.chain)
                .await?;
        tracing::info!(uid=%self.uid, chain=%self.chain, rows=%affected, "Recover: reset unfinished to pending");

        // 2️⃣ 以 item 为准，补齐 batch finished_count（可选但强烈建议）
        tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: recompute finished count");
        let affected =
            ExpandBatchRepo::recompute_finished_count(pool.clone(), &self.uid, &self.chain).await?;
        tracing::info!(uid=%self.uid, chain=%self.chain, rows=%affected, "Recover: recompute finished count");

        // 4️⃣ 再 dispatch notify for done batches
        self.dispatch_notify_for_done_batches().await?;

        Ok(())
    }

    async fn handle_schedule(&mut self) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "Schedule: start inner");
        self.handle_schedule_inner().await
    }

    async fn handle_schedule_inner(&mut self) -> Result<(), ServiceError> {
        if !matches!(self.address_sync, AddressSyncState::Done) {
            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                "Schedule skipped: address sync not done"
            );
            return Ok(());
        }

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        // 1️⃣ 统计 inflight 数量（Creating）
        let inflight =
            ExpandBatchItemRepo::count_inflight(pool.clone(), &self.uid, &self.chain).await?;
        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            inflight=%inflight,
            "expand schedule count inflight"
        );
        let quota = EXPAND_MAX_INFLIGHT.saturating_sub(inflight as usize);
        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            quota=%quota,
            "expand schedule quota"
        );
        self.reload_existing_from_db().await?;
        if quota == 0 {
            return Ok(());
        }

        // 2️⃣ 取 Pending items
        let items = ExpandBatchItemRepo::fetch_retryable(
            pool.clone(),
            &self.uid,
            &self.chain,
            quota as i64,
        )
        .await?;
        tracing::info!(
            uid = %self.uid,
            chain = %self.chain,
            count = items.len(),
            "expand schedule fetched items"
        );
        if items.is_empty() {
            return Ok(());
        }

        let failed_indices: Vec<i32> = items
            .iter()
            .filter(|it| it.status == ExpandItemStatus::Failed)
            .map(|it| it.input_index)
            .collect();

        if !failed_indices.is_empty() {
            ExpandBatchItemRepo::mark_items_status_by_owner_from(
                pool.clone(),
                &self.uid,
                &self.chain,
                &failed_indices,
                &[ExpandItemStatus::Failed],
                ExpandItemStatus::Pending,
            )
            .await?;
        }

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
                ExpandBatchItemRepo::mark_items_status_from(
                    pool.clone(),
                    &batch_id,
                    &to_create,
                    ExpandItemStatus::Pending,
                    ExpandItemStatus::Creating,
                )
                .await?;

                crate::infrastructure::expand_address::worker::WORKER_POOL
                    .tx
                    .send(ExpandJob::Create {
                        uid: self.uid.clone(),
                        chain: self.chain.clone(),
                        batch_id: batch_id.clone(),
                        indices: to_create.clone(),
                    })
                    .await
                    .map_err(|e| {
                        ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                            e.to_string(),
                        ))
                    })?;
            }

            if !to_init.is_empty() {
                ExpandBatchItemRepo::mark_items_status_from(
                    pool.clone(),
                    &batch_id,
                    &to_init,
                    ExpandItemStatus::Pending,
                    ExpandItemStatus::Initing,
                )
                .await?;
                crate::infrastructure::expand_address::worker::WORKER_POOL
                    .tx
                    .send(ExpandJob::Init {
                        uid: self.uid.clone(),
                        chain: self.chain.clone(),
                        batch_id: batch_id.clone(),
                        indices: to_init.clone(),
                    })
                    .await
                    .map_err(|e| {
                        ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                            e.to_string(),
                        ))
                    })?;
            }
        }
        // // 5️⃣ 再 dispatch notify for done batches
        // self.dispatch_notify_for_done_batches().await?;
        Ok(())
    }

    async fn handle_recover_expand_items(
        &mut self,
        batch_id: &str,
        missing: u32,
    ) -> Result<(), ServiceError> {
        if missing == 0 {
            return Ok(());
        }

        tracing::info!(
            uid = %self.uid,
            chain = %self.chain,
            batch_id = %batch_id,
            missing = missing,
            "recover expand:补建缺失的 items"
        );

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let used = AwmCmdAddrExpandMsg::collect_used_indices(&self.uid, &self.chain).await?;
        let indices = AwmCmdAddrExpandMsg::allocate_indices(&used, missing);

        if indices.is_empty() {
            return Ok(());
        }

        // 2️⃣ 直接补建 batch items（状态 = Pending）
        ExpandBatchItemRepo::batch_create_items(
            pool.clone(),
            &self.uid,
            batch_id,
            &self.chain,
            &indices,
        )
        .await?;

        // 3️⃣ 触发一次调度即可
        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;

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

        ExpandBatchItemRepo::batch_create_items(
            pool.clone(),
            &self.uid,
            &msg.batch_id,
            &self.chain,
            &needed,
        )
        .await?;

        tracing::info!(
            task_id=%task_id,
            uid=%self.uid,
            chain=%self.chain,
            batch_id=%msg.batch_id,
            "handle_new_expand: batch items created, triggering schedule"
        );

        // self.handle_recover_task(&task_id, &msg.batch_id).await
        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;

        tracing::info!(
            task_id=%task_id,
            uid=%self.uid,
            chain=%self.chain,
            batch_id=%msg.batch_id,
            "handle_new_expand: completed"
        );

        Ok(())
    }

    async fn handle_account_created(&mut self, indices: Vec<i32>) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        tracing::info!(
            uid=%self.uid, chain=%self.chain, indices=?indices,
            "accounts created, mark Initing"
        );

        ExpandBatchItemRepo::mark_items_status_by_owner_from(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
            &[ExpandItemStatus::Creating],
            ExpandItemStatus::Initing,
        )
        .await?;

        // 重新加载 existing_indices，保证后续 schedule 判断准确
        self.reload_existing_from_db().await?;

        // 直接推进调度（不必等下次扫表）
        self.self_sender
            .send(ExpandActorMsg::Schedule)
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;

        Ok(())
    }

    async fn handle_address_inited(
        &mut self,
        indices: Vec<i32>, // 修改为接受索引数组
    ) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            indices=?indices,
            count=%indices.len(),
            "handle_address_inited: start processing"
        );

        if indices.is_empty() {
            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                "handle_address_inited: empty indices, skip processing"
            );
            return Ok(());
        }

        let before = ExpandBatchItemRepo::list_status_by_indices(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
        )
        .await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            indices=?indices,
            before_status=?before,
            "handle_address_inited: status before mark done"
        );

        let updated = ExpandBatchItemRepo::mark_items_done_by_owner(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
            &[
                ExpandItemStatus::Initing,
                ExpandItemStatus::Creating,
                ExpandItemStatus::Pending,
                ExpandItemStatus::Failed,
            ],
        )
        .await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            indices=?indices,
            rows=%updated,
            "handle_address_inited: marked items Done"
        );

        ExpandBatchRepo::recompute_finished_count(pool.clone(), &self.uid, &self.chain).await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            "handle_address_inited: recomputed finished count"
        );

        // 3️⃣ 推进 finished >= total 的 batch 为 Done
        let done_batches =
            ExpandBatchRepo::get_all_finished_but_running(pool.clone(), &self.uid, &self.chain)
                .await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            done_batches_count=%done_batches.len(),
            done_batches=?done_batches.iter().map(|b| &b.batch_id).collect::<Vec<_>>(),
            "handle_address_inited: found finished but running batches"
        );

        for b in done_batches {
            let updated = ExpandBatchRepo::mark_done_if_finished(pool.clone(), &b.batch_id).await?;

            if updated {
                tracing::info!(
                    uid=%self.uid,
                    chain=%self.chain,
                    batch_id=%b.batch_id,
                    "handle_address_inited: batch completed and marked Done"
                );
            }
            self.dispatch_notify_for_done_batches().await?;
        }

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            indices=?indices,
            "handle_address_inited: processing completed, triggering schedule"
        );

        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;
        Ok(())
    }

    async fn handle_notify_address_expanded(
        &mut self,
        batch_id: String,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            batch_id=%batch_id,
            "处理地址扩容通知完成（批量）"
        );
        if ExpandBatchRepo::mark_as_notified(pool.clone(), &batch_id).await? {
            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                batch_id=%batch_id,
                "标记地址扩容批次为已通知"
            );
        }
        // 再触发一次 schedule，看看还有没有可推进的
        self.self_sender
            .send(ExpandActorMsg::Schedule)
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;

        Ok(())
    }

    async fn handle_job_failed(
        &mut self,
        phase: ExpandItemStatus,
        indices: Vec<i32>,
        error: String,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        tracing::warn!(
            uid=%self.uid,
            chain=%self.chain,
            phase=?phase,
            indices=?indices,
            count=%indices.len(),
            error=%error,
            "handle_job_failed: expand job failed"
        );

        ExpandBatchItemRepo::mark_failed_and_inc_retry(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
            phase,
        )
        .await?;

        self.self_sender
            .send(ExpandActorMsg::Schedule)
            .await
            .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;

        Ok(())
    }

    async fn reload_existing_from_db(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let ex: Vec<u32> =
            ApiAccountRepo::get_all_account_indices(pool.clone(), &self.uid, &self.chain).await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            account_ids=?ex,
            "reload_existing_from_db: loading from DB"
        );

        self.existing_indices = ex
            .into_iter()
            .map(|id| {
                // account id -> input_index
                wallet_utils::address::AccountIndexMap::from_account_id(id)
                    .map(|m| m.input_index)
                    .unwrap_or_default()
            })
            .collect();

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            existing_indices=?self.existing_indices,
            "reload_existing_from_db: reloaded existing indices"
        );

        Ok(())
    }

    async fn dispatch_notify_for_done_batches(&self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let done = ExpandBatchRepo::get_all_done_but_not_notified(pool.clone()).await?;

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            done_batches_count=%done.len(),
            done_batches=?done.iter().map(|b| &b.batch_id).collect::<Vec<_>>(),
            "dispatch_notify_for_done_batches: start processing"
        );

        for b in done {
            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                batch_id=%b.batch_id,
                "dispatch_notify_for_done_batches: sending notify job"
            );

            crate::infrastructure::expand_address::worker::WORKER_POOL
                .tx
                .send(ExpandJob::Notify {
                    uid: self.uid.clone(),
                    chain: self.chain.clone(),
                    batch_id: b.batch_id.clone(),
                })
                .await
                .map_err(|e| ServiceError::System(SystemError::ChannelSendFailed(e.to_string())))?;

            tracing::info!(
                uid=%self.uid,
                chain=%self.chain,
                batch_id=%b.batch_id,
                "dispatch_notify_for_done_batches: notify job sent"
            );
        }

        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            "dispatch_notify_for_done_batches: completed"
        );

        Ok(())
    }
}
