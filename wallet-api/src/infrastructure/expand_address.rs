// expand_actor.rs
// Actor-based expand address manager for your wallet system.
// - Supervisor manages per-(uid,chain) ExpandActor
// - Each ExpandActor runs in a single tokio task and serializes all operations
// - On startup, supervisor can recover unfinished tasks from TaskQueueRepo
// - ADDRESS_INIT events and incoming expand tasks are sent to the actor

use serde::{Deserialize, Serialize};
use wallet_database::entities::task_queue::{KnownTaskName, TaskName};
use wallet_database::repositories::api_wallet::account::ApiAccountRepo;
use wallet_database::repositories::task_queue::TaskQueueRepo;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, oneshot};
use once_cell::sync::Lazy;

use crate::context::CONTEXT;
use crate::domain::api_wallet::account::ApiAccountDomain;
use crate::domain::api_wallet::wallet::ApiWalletDomain;
use crate::messaging::mqtt::topics::api_wallet::cmd::address_allock::{AwmCmdAddrExpandMsg, ExpandStatus};

use crate::error::service::ServiceError;
use wallet_transport_backend::request::api_wallet::address::{ApiAddressInitReq, ExpandAddressCompleteReq};

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
        index: i32,
    },
    /// Recover existing task (used on startup)
    RecoverTask {
        task_id: String,
        status: ExpandStatus,
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
        self.sender.send(msg).await.map_err(|_| ServiceError::Internal("actor closed".into()))
    }
}

// Supervisor which holds actor handles
type ActorMap = Arc<Mutex<HashMap<(String, String), ExpandActorHandle>>>;

static SUPERVISOR: Lazy<ActorMap> = Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Get or create an actor for (uid, chain). This will spawn the actor task if necessary.
pub async fn get_or_create_actor(uid: &str, chain: &str) -> ExpandActorHandle {
    let key = (uid.to_string(), chain.to_string());
    let mut map = SUPERVISOR.lock().await;
    if let Some(handle) = map.get(&key) {
        return handle.clone();
    }

    let (tx, rx) = mpsc::channel(ACTOR_CHANNEL_SIZE);
    let handle = ExpandActorHandle { sender: tx.clone() };

    // spawn actor
    let uid_c = uid.to_string();
    let chain_c = chain.to_string();
    tokio::spawn(async move {
        if let Err(e) = ExpandActor::new(uid_c.clone(), chain_c.clone()).run(rx).await {
            tracing::error!("expand actor {}|{} exited with error: {:?}", uid_c, chain_c, e);
        }
    });

    map.insert(key, handle.clone());
    handle
}

/// Initialize supervisor: recover unfinished expand tasks into actors
pub async fn init_expand_supervisor() -> Result<(), ServiceError> {
    let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;

    // find tasks with name AwmCmdAddrExpand in states pending/inprogress/failed
    let tasks = TaskQueueRepo::list_tasks_with_task_name(&pool, TaskName::Known(KnownTaskName::AwmCmdAddrExpand), &[0,1,3]).await?;

    for task in tasks {
        if let Some(remark) = task.remark {
            // try load remark, fallback to constructing from request body
            match ExpandStatus::load_or_fix_remark(&task).await {
                Ok(status) => {
                    let actor = get_or_create_actor(&status.uid, &status.chain_code.unwrap_or_default()).await;
                    let _ = actor.send(ExpandActorMsg::RecoverTask { task_id: task.id.clone(), status }).await;
                }
                Err(err) => tracing::warn!("failed to load remark for recover task {}: {:?}", task.id, err),
            }
        }
    }

    Ok(())
}

// The actor state and implementation
struct ExpandActor {
    uid: String,
    chain: String,
    // indices that already have an account row (from DB)
    existing_indices: HashSet<i32>,
    // indices that are waiting to be created/initialized
    needed_indices: HashSet<i32>,
    // indices that have been initialized (init reported)
    completed_indices: HashSet<i32>,
    // map serial_no -> indices for that expand task
    serial_map: HashMap<String, HashSet<i32>>,
    // mapping task_id -> serial_no (so we can update TaskQueue remark)
    task_to_serial: HashMap<String, String>,
}

impl ExpandActor {
    pub async fn new(uid: String, chain: String) -> Self {
        // load existing indices & completed indices from DB
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool().unwrap();

        let existing = ApiAccountRepo::get_all_account_indices(&pool, &uid, &chain).await
            .unwrap_or_default()
            .into_iter()
            .map(|id| {
                // account id -> input_index
                wallet_utils::address::AccountIndexMap::from_account_id(id)
                    .map(|m| m.input_index)
                    .unwrap_or_default()
            })
            .collect::<HashSet<i32>>();

        let completed = ApiAccountRepo::list_inited_indices(&pool, &uid).await
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.0)
            .collect::<HashSet<i32>>();

        ExpandActor {
            uid,
            chain,
            existing_indices: existing,
            needed_indices: HashSet::new(),
            completed_indices: completed,
            serial_map: HashMap::new(),
            task_to_serial: HashMap::new(),
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<ExpandActorMsg>) -> Result<(), ServiceError> {
        while let Some(msg) = rx.recv().await {
            match msg {
                ExpandActorMsg::NewExpandTask { task_id, msg, reply } => {
                    let r = self.handle_new_expand(task_id.clone(), msg).await;
                    if let Some(tx) = reply {
                        let _ = tx.send(r.map_err(|e| e));
                    }
                }
                ExpandActorMsg::AddressInited { task_ids, uid: _, chain: _, index } => {
                    if let Err(e) = self.handle_address_inited(task_ids, index).await {
                        tracing::error!("handle_address_inited error: {:?}", e);
                    }
                }
                ExpandActorMsg::RecoverTask { task_id, status } => {
                    if let Err(e) = self.handle_recover_task(task_id, status).await {
                        tracing::error!("recover task error: {:?}", e);
                    }
                }
                ExpandActorMsg::Shutdown => {
                    tracing::info!("shutting down actor {}|{}", self.uid, self.chain);
                    break;
                }
            }
        }
        Ok(())
    }

    /// handle incoming expand task
    async fn handle_new_expand(&mut self, task_id: String, msg: AwmCmdAddrExpandMsg) -> Result<(), ServiceError> {
        // compute needed indices using your helper
        let needed = AwmCmdAddrExpandMsg::get_needed_indices(&msg.typ, &self.chain, msg.number, msg.index, &self.uid).await?;

        // filter out existing or already completed
        let new_needed: Vec<i32> = needed.into_iter()
            .filter(|i| !self.existing_indices.contains(i))
            .filter(|i| !self.completed_indices.contains(i))
            .collect();

        if new_needed.is_empty() {
            // nothing to do: mark task remark/status accordingly
            // load or build remark and set status true
            let mut remark = ExpandStatus::from_simple(&self.uid, &new_needed, msg.serial_no.clone());
            remark.completed_indices = self.completed_indices.clone();
            remark.status = remark.needed_indices.iter().all(|i| remark.completed_indices.contains(i));
            let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
            TaskQueueRepo::update_task_remark(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, &task_id, &updated).await?;
            TaskQueueRepo::update_task_state(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, &task_id, 2).await?;
            // also call expand_address_complete if status true
            if remark.status {
                let backend = CONTEXT.get().unwrap().get_global_backend_api();
                let req = ExpandAddressCompleteReq::new(&self.uid, &msg.serial_no, true, None);
                backend.expand_address_complete(req).await?;
            }
            return Ok(());
        }

        // register serial mapping
        self.serial_map.entry(msg.serial_no.clone()).or_default().extend(new_needed.iter().copied());
        self.task_to_serial.insert(task_id.clone(), msg.serial_no.clone());

        // persist remark for this task (so recovery can rebuild if process dies)
        let mut remark = ExpandStatus::new(&self.uid, &new_needed, self.completed_indices.clone(), false, new_needed.len() as u32, &msg.serial_no);
        let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
        TaskQueueRepo::update_task_remark(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, &task_id, &updated).await?;

        // actually create sub accounts (this will insert rows into DB but NOT mark init)
        let password = ApiWalletDomain::get_passwd().await?;
        ApiAccountDomain::create_sub_account(
            &self.uid,
            &self.uid,
            &password,
            &self.chain,
            "账户",
            true,
            new_needed.len() as u32,
            new_needed.clone(),
        ).await?;

        // after create_sub_account returns, new rows should exist in DB (but not yet inited)
        // update existing_indices from DB
        self.reload_existing_from_db().await?;

        // no immediate init; ADDRESS_INIT will arrive externally and feed AddressInited messages
        Ok(())
    }

    async fn handle_recover_task(&mut self, task_id: String, status: ExpandStatus) -> Result<(), ServiceError> {
        // merge remark data into actor state
        // ensure we only keep indices that are relevant
        let needed: HashSet<i32> = status.needed_indices.into_iter().collect();
        let completed: HashSet<i32> = status.completed_indices.into_iter().collect();

        // merge
        for i in &needed { self.needed_indices.insert(*i); }
        for i in &completed { self.completed_indices.insert(*i); }

        // record serial mapping
        self.serial_map.entry(status.serial_no.clone()).or_default().extend(needed.iter().copied());
        self.task_to_serial.insert(task_id.clone(), status.serial_no.clone());

        // also ensure existing_indices updated
        self.reload_existing_from_db().await?;

        // check if some serial already done
        self.check_and_complete_serials().await?;

        Ok(())
    }

    async fn handle_address_inited(&mut self, task_ids: Vec<String>, index: i32) -> Result<(), ServiceError> {
        // update completed & needed
        self.completed_indices.insert(index);
        self.needed_indices.remove(&index);

        // update DB-backed remark for tasks that reference this serial
        // find affected serials
        let mut affected_serials = vec![];
        for (serial, set) in &self.serial_map {
            if set.contains(&index) {
                affected_serials.push(serial.clone());
            }
        }

        // update remark in DB for task_ids referencing serials
        for task_id in task_ids.iter().chain(self.task_to_serial.keys()) {
            if let Some(serial) = self.task_to_serial.get(task_id) {
                if let Some(indices) = self.serial_map.get(serial) {
                    let mut remark = ExpandStatus::new(&self.uid, &indices.iter().copied().collect::<Vec<_>>(), self.completed_indices.clone(), false, indices.len() as u32, serial);
                    remark.status = remark.needed_indices.iter().all(|i| remark.completed_indices.contains(i));
                    let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
                    TaskQueueRepo::update_task_remark(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, task_id, &updated).await?;
                    if remark.status {
                        TaskQueueRepo::update_task_state(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, task_id, 2).await?;
                        // call backend
                        let backend = CONTEXT.get().unwrap().get_global_backend_api();
                        let req = ExpandAddressCompleteReq::new(&self.uid, serial, true, None);
                        backend.expand_address_complete(req).await?;
                    }
                }
            }
        }

        // check serials for completion and trigger callbacks
        self.check_and_complete_serials().await?;

        Ok(())
    }

    async fn reload_existing_from_db(&mut self) -> Result<(), ServiceError> {
        let pool = CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let ex = ApiAccountRepo::get_all_account_indices(&pool, &self.uid, &self.chain).await?;
        self.existing_indices = ex.into_iter().map(|id| wallet_utils::address::AccountIndexMap::from_account_id(id).map(|m| m.input_index).unwrap_or_default()).collect();
        Ok(())
    }

    async fn check_and_complete_serials(&mut self) -> Result<(), ServiceError> {
        let completed_serials: Vec<String> = self.serial_map.iter()
            .filter(|(serial, set)| set.iter().all(|i| self.completed_indices.contains(i)))
            .map(|(s, _)| s.clone())
            .collect();

        for serial in completed_serials {
            // remove mapping
            if let Some(indices) = self.serial_map.remove(&serial) {
                // find associated task_ids
                let task_ids: Vec<String> = self.task_to_serial.iter().filter_map(|(tid, s)| if s == &serial { Some(tid.clone()) } else { None }).collect();

                // update tasks remarks & states
                for task_id in task_ids.iter() {
                    let mut remark = ExpandStatus::new(&self.uid, &indices.iter().copied().collect::<Vec<_>>(), self.completed_indices.clone(), true, indices.len() as u32, &serial);
                    remark.status = true;
                    let updated = wallet_utils::serde_func::serde_to_string(&remark)?;
                    TaskQueueRepo::update_task_remark(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, task_id, &updated).await?;
                    TaskQueueRepo::update_task_state(&CONTEXT.get().unwrap().get_global_sqlite_pool()?, task_id, 2).await?;
                }

                // call backend to notify completion
                let backend = CONTEXT.get().unwrap().get_global_backend_api();
                let req = ExpandAddressCompleteReq::new(&self.uid, &serial, true, None);
                backend.expand_address_complete(req).await?;

                // remove task_to_serial entries for completed serial
                self.task_to_serial.retain(|_, s| s != &serial);
            }
        }

        Ok(())
    }
}

// ===== Helper APIs for external use =====

/// Submit a new expand task to the actor system
pub async fn submit_expand_task(task_id: String, msg: AwmCmdAddrExpandMsg) -> Result<(), ServiceError> {
    let actor = get_or_create_actor(&msg.uid, &msg.chain_code).await;
    let (tx, rx) = oneshot::channel();
    actor.send(ExpandActorMsg::NewExpandTask { task_id, msg, reply: Some(tx) }).await?;
    rx.await
    .map_err(|_| ServiceError::Internal("actor reply dropped".into()))??;
    Ok(())
}

/// Called from ADDRESS_INIT handler to let actor know an index has been inited
pub async fn submit_address_inited(uid: String, chain: String, index: i32, related_task_ids: Vec<String>) -> Result<(), ServiceError> {
    let actor = get_or_create_actor(&uid, &chain).await;
    actor.send(ExpandActorMsg::AddressInited { task_ids: related_task_ids, uid, chain, index }).await?;
    Ok(())
}
