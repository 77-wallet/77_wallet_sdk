// facade.rs
/// 决定能不能进入扩容系统
use once_cell::sync::Lazy;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::{
    infrastructure::expand_address::actor::{ExpandActor, ExpandActorHandle, ExpandActorMsg},
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AwmCmdAddrExpandMsg,
};

use crate::error::service::ServiceError;

// Supervisor which holds actor handles
type ActorMap = Arc<Mutex<HashMap<ActorKey, ExpandActorHandle>>>;

static SUPERVISOR: Lazy<ActorMap> = Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

pub(crate) const ACTOR_CHANNEL_SIZE: usize = 256;

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

pub struct ExpandAddressFacade;

impl ExpandAddressFacade {
    /// Get or create an actor for (uid, chain). This will spawn the actor task if necessary.
    pub async fn get_or_create_actor(
        uid: &str,
        chain: &str,
    ) -> Result<ExpandActorHandle, ServiceError> {
        let key = ActorKey::from((uid, chain));

        // 1️⃣ 先快速检查
        if let Some(handle) = {
            let map = SUPERVISOR.lock().await;
            map.get(&key).cloned()
        } {
            return Ok(handle);
        }

        // 2️⃣ 不持锁，准备 actor
        let (tx, rx) = mpsc::channel(ACTOR_CHANNEL_SIZE);
        let handle = ExpandActorHandle { sender: tx.clone() };

        let uid_c = uid.to_string();
        let chain_c = chain.to_string();

        // 3️⃣ 再次加锁，防止并发重复创建
        let mut map = SUPERVISOR.lock().await;
        if let Some(existing) = map.get(&key) {
            // 已被别人创建，丢弃刚刚这个
            return Ok(existing.clone());
        }

        tokio::task::spawn(async move {
            let actor = ExpandActor::new(uid_c.clone(), chain_c.clone(), tx);

            if let Err(e) = actor.run(rx).await {
                tracing::error!("expand actor {}|{} exited with error: {:?}", uid_c, chain_c, e);
            }
        });

        map.insert(key, handle.clone());
        Ok(handle)
    }

    // ===== Helper APIs for external use =====

    /// Submit a new expand task to the actor system
    pub async fn submit_expand_task(
        task_id: String,
        msg: AwmCmdAddrExpandMsg,
    ) -> Result<(), ServiceError> {
        tracing::info!("submit_expand_task -------------- 1");
        let actor: ExpandActorHandle = Self::get_or_create_actor(&msg.uid, &msg.chain_code).await?;
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

    /// Called from ADDRESS_INIT handler to let actor know an index has been inited
    pub async fn submit_address_inited(
        uid: &str,
        chain: &str,
        indices: Vec<i32>, // 修改为接受索引数组
    ) -> Result<(), ServiceError> {
        let actor: ExpandActorHandle = Self::get_or_create_actor(uid, chain).await?;
        actor.send(ExpandActorMsg::AddressInited { indices }).await?;
        Ok(())
    }

    /// Called from ACCOUNT_CREATED handler to let actor know an index has been created
    pub async fn submit_account_created(
        uid: &str,
        chain: &str,
        indices: Vec<i32>, // 修改为接受索引数组
    ) -> Result<(), ServiceError> {
        let actor: ExpandActorHandle = Self::get_or_create_actor(uid, chain).await?;
        actor.send(ExpandActorMsg::AccountCreated { indices }).await?;
        Ok(())
    }

    /// Called from ACCOUNT_EXPANDED handler to let actor know an index has been expanded
    pub async fn submit_address_expanded(
        uid: &str,
        chain: &str,
        batch_id: &str, // 修改为接受批次ID
    ) -> Result<(), ServiceError> {
        let actor: ExpandActorHandle = Self::get_or_create_actor(uid, chain).await?;
        actor
            .send(ExpandActorMsg::NotifyAddressExpanded { batch_id: batch_id.to_string() })
            .await?;
        Ok(())
    }

    /// Called from ACCOUNT_EXPANDED handler to let actor know backend address sync is done
    pub async fn submit_backend_address_synced(uid: &str, chain: &str) -> Result<(), ServiceError> {
        let actor: ExpandActorHandle = Self::get_or_create_actor(uid, chain).await?;
        tracing::info!(
            "submit_backend_address_synced get_or_create_actor uid={} chain={}",
            uid,
            chain
        );
        actor.send(ExpandActorMsg::BackendAddressSynced).await?;
        Ok(())
    }

    /// Called from ACCOUNT_EXPANDED handler to let actor know backend address sync is done
    pub async fn submit_backend_address_syncing(
        uid: &str,
        chain: &str,
    ) -> Result<(), ServiceError> {
        let actor: ExpandActorHandle = Self::get_or_create_actor(uid, chain).await?;
        tracing::info!(
            "submit_backend_address_syncing get_or_create_actor uid={} chain={}",
            uid,
            chain
        );
        actor.send(ExpandActorMsg::BackendAddressSyncing).await?;
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
}
