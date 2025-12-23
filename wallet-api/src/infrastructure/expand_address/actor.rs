use std::collections::{BTreeSet, HashMap};
use tokio::sync::{mpsc, oneshot};
use wallet_database::{
    entities::expand_batch_item::ExpandItemStatus,
    repositories::api_wallet::{
        account::ApiAccountRepo, expand_batch::ExpandBatchRepo,
        expand_batch_item::ExpandBatchItemRepo,
    },
};
use wallet_utils::address::AccountIndexMap;

use crate::{
    error::{service::ServiceError, system::SystemError},
    infrastructure::expand_address::{facade::ExpandAddressFacade, worker::ExpandJob},
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
    /// Recover existing task (used on startup)
    RecoverTask {
        reply: Option<oneshot::Sender<Result<(), ServiceError>>>,
    },
    /// Schedule a check for completed batches
    Schedule,
    /// Shutdown actor
    Shutdown,
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
// The actor state and implementation
#[derive(Debug)]
pub(crate) struct ExpandActor {
    uid: String,
    chain: String,
    // indices that already have an account row (from DB)
    existing_indices: BTreeSet<i32>,
    scheduling: bool,
    schedule_pending: bool,
    self_sender: mpsc::Sender<ExpandActorMsg>,
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
        }
    }

    async fn load_existing_indices(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let existing_accounts =
            ApiAccountRepo::get_all_account_indices(pool.clone(), &self.uid, &self.chain).await?;

        self.existing_indices = existing_accounts
            .into_iter()
            .map(|id| {
                AccountIndexMap::from_account_id(id).map(|m| m.input_index).unwrap_or_default()
            })
            .collect();

        Ok(())
    }

    pub(crate) async fn run(
        mut self,
        mut rx: mpsc::Receiver<ExpandActorMsg>,
    ) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor started");
        self.load_existing_indices().await?;
        while let Some(msg) = rx.recv().await {
            match msg {
                ExpandActorMsg::NewExpandTask { task_id, msg, reply } => {
                    let r = self.handle_new_expand(task_id.clone(), msg).await;
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
                ExpandActorMsg::RecoverTask { reply } => {
                    tracing::info!(uid=%self.uid, chain=%self.chain, "Recover: reset unfinished items");
                    let r = self.recover().await;

                    if let Some(tx) = reply {
                        let _ = tx.send(r);
                    }
                }
                ExpandActorMsg::Schedule => {
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
            }
        }

        tracing::info!(uid=%self.uid, chain=%self.chain, "ExpandActor stopped");
        Ok(())
    }

    async fn recover(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        // 1️⃣ Failed / Creating / Initing → Pending
        let affected =
            ExpandBatchItemRepo::reset_unfinished_to_pending(pool.clone(), &self.uid, &self.chain)
                .await?;
        // 2️⃣ 以 item 为准，补齐 batch finished_count（可选但强烈建议）
        ExpandBatchRepo::recompute_finished_count(pool.clone(), &self.uid, &self.chain).await?;
        // 3️⃣ 补完成 batch
        self.check_and_complete_batches().await?;
        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            rows=%affected,
            "Recover: items reset to Pending"
        );
        // 4️⃣ 再 schedule 推进 Pending
        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;
        Ok(())
    }

    async fn handle_schedule(&mut self) -> Result<(), ServiceError> {
        tracing::info!(uid=%self.uid, chain=%self.chain, "Schedule: start inner");
        self.handle_schedule_inner().await
    }

    async fn handle_schedule_inner(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        // 1️⃣ 统计 inflight 数量（Creating / Initing）
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
        let items =
            ExpandBatchItemRepo::fetch_pending(pool.clone(), &self.uid, &self.chain, quota as i64)
                .await?;
        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            count=%items.len(),
            "expand schedule fetched items"
        );
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
                ExpandBatchItemRepo::mark_items_status_from(
                    pool.clone(),
                    &batch_id,
                    &to_create,
                    ExpandItemStatus::Pending,
                    ExpandItemStatus::Creating,
                )
                .await?;

                let res = crate::infrastructure::expand_address::worker::WORKER_POOL
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
                        pool.clone(),
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
                    pool.clone(),
                    &batch_id,
                    &to_init,
                    ExpandItemStatus::Pending,
                    ExpandItemStatus::Initing,
                )
                .await?;
                let res = crate::infrastructure::expand_address::worker::WORKER_POOL
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
                        pool.clone(),
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

    /// 程序启动时调用：恢复所有未完成的 expand 批次
    pub async fn recover_unfinished_expand_items() -> Result<(), ServiceError> {
        tracing::info!("开始 recover 未完成的 expand items");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;

        let batches = ExpandBatchRepo::get_unfinished_batches(pool.clone()).await?;
        tracing::info!("发现未完成批次数量: {}", batches.len());

        let mut seen = std::collections::HashSet::new();

        for b in batches {
            let key = (b.uid.clone(), b.chain_code.clone());
            // 同一个 uid+chain 只需要 recover 一次
            if !seen.insert(key.clone()) {
                continue;
            }

            let actor = ExpandAddressFacade::get_or_create_actor(&b.uid, &b.chain_code).await?;
            actor.send(ExpandActorMsg::RecoverTask { reply: None }).await?;

            tracing::info!(
                uid=%b.uid,
                chain=%b.chain_code,
                "已发送 RecoverTask"
            );
        }

        Ok(())
    }

    /// 恢复未完成的expand_address_complete操作
    /// 程序启动时调用，检查所有AwmCmdAddrExpand任务，找出那些地址已全部初始化但未发送完成通知的任务
    pub async fn recover_unfinished_expand_complete() -> Result<(), ServiceError> {
        tracing::info!("开始恢复未完成的地址扩展完成操作");

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let backend = crate::context::get_context()?.get_global_backend_api();

        let done = ExpandBatchRepo::get_all_done_but_not_notified(pool.clone()).await?;

        let mut recovered = 0;

        for batch in done {
            // backend
            //     .expand_address_complete(ExpandAddressCompleteReq::new(
            //         &batch.uid,
            //         &batch.batch_id,
            //         &batch.serial_no,
            //         true,
            //         None,
            //     ))
            //     .await?;
            tracing::info!(
                uid=%batch.uid,
                chain=%batch.chain_code,
                batch_id=%batch.batch_id,
                serial_no=%batch.serial_no,
                "已恢复地址扩展完成批次"
            );

            ExpandBatchRepo::mark_as_notified(pool.clone(), &batch.batch_id).await?;
            recovered += 1;
        }
        tracing::info!("地址扩展完成恢复结束，共恢复 {} 个批次", recovered);
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
            pool.clone(),
            &self.uid,
            &msg.batch_id,
            &msg.serial_no,
            &self.chain,
            needed.len() as i32,
        )
        .await?;
        ExpandBatchItemRepo::batch_create_items(
            pool.clone(),
            &self.uid,
            &msg.batch_id,
            &self.chain,
            &needed,
        )
        .await?;

        // self.handle_recover_task(&task_id, &msg.batch_id).await
        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;
        Ok(())
    }

    async fn handle_account_created(&mut self, indices: Vec<i32>) -> Result<(), ServiceError> {
        tracing::info!(
            uid=%self.uid, chain=%self.chain, indices=?indices,
            "accounts created, reload existing and reschedule"
        );

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

        tracing::info!(uid=%self.uid, chain=%self.chain, indices=?indices, "处理地址初始化完成（批量）");
        if indices.is_empty() {
            return Ok(());
        }

        let before = ExpandBatchItemRepo::list_status_by_indices(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
        )
        .await?;
        tracing::info!(?before, "status before mark done");
        let updated = ExpandBatchItemRepo::mark_items_done_by_owner(
            pool.clone(),
            &self.uid,
            &self.chain,
            &indices,
        )
        .await?;
        tracing::info!(
            uid=%self.uid,
            chain=%self.chain,
            rows=%updated,
            "ADDRESS_INIT: marked items Done"
        );

        ExpandBatchRepo::recompute_finished_count(pool.clone(), &self.uid, &self.chain).await?;

        // 3️⃣ 推进 finished >= total 的 batch 为 Done
        let done_batches =
            ExpandBatchRepo::get_all_finished_but_running(pool.clone(), &self.uid, &self.chain)
                .await?;

        for b in done_batches {
            let updated = ExpandBatchRepo::mark_done_if_finished(pool.clone(), &b.batch_id).await?;

            if updated {
                tracing::info!(
                    uid=%self.uid,
                    chain=%self.chain,
                    batch_id=%b.batch_id,
                    "批次已完成并推进为 Done"
                );
            }
        }

        self.self_sender.send(ExpandActorMsg::Schedule).await.map_err(|e| {
            ServiceError::System(crate::error::system::SystemError::ChannelSendFailed(
                e.to_string(),
            ))
        })?;
        Ok(())
    }

    async fn reload_existing_from_db(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let ex: Vec<u32> =
            ApiAccountRepo::get_all_account_indices(pool.clone(), &self.uid, &self.chain).await?;
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
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let batches =
            ExpandBatchRepo::get_all_finished_but_running(pool.clone(), &self.uid, &self.chain)
                .await?;
        // let backend = CONTEXT.get().unwrap().get_global_backend_api();
        for batch in batches {
            // 标记为已完成
            if ExpandBatchRepo::mark_done_if_finished(pool.clone(), &batch.batch_id).await? {
                // backend
                //     .expand_address_complete(ExpandAddressCompleteReq::new(
                //         &self.uid,
                //         &batch.batch_id,
                //         &batch.serial_no,
                //         true,
                //         None,
                //     ))
                //     .await?;
                tracing::info!(
                    uid=%self.uid,
                    chain=%self.chain,
                    batch_id=%batch.batch_id,
                    "已完成地址扩容批次"
                );
                // 标记为已通知
                if ExpandBatchRepo::mark_as_notified(pool.clone(), &batch.batch_id).await? {
                    tracing::info!(
                        uid=%self.uid,
                        chain=%self.chain,
                        batch_id=%batch.batch_id,
                        "标记地址扩容批次为已通知"
                    );
                }
            } else {
                tracing::info!(
                    uid=%self.uid,
                    chain=%self.chain,
                    batch_id=%batch.batch_id,
                    "标记为已完成失败"
                );
            }
        }

        Ok(())
    }
}
