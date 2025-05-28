use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use tokio::sync::Notify;

use crate::service::asset::AssetsService;

pub type InnerEventSender = tokio::sync::mpsc::UnboundedSender<InnerEvent>;

pub enum InnerEvent {
    SyncAssets {
        addr_list: Vec<String>,
        chain_code: String,
        symbol: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AssetKey {
    address: String,
    chain_code: String,
    symbol: String,
}

impl From<(&str, &str, &str)> for AssetKey {
    fn from((address, chain_code, symbol): (&str, &str, &str)) -> Self {
        Self {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
        }
    }
}

struct EventBuffer {
    buffer: Arc<Mutex<HashSet<AssetKey>>>,
    notifier: Arc<Notify>,
}

impl EventBuffer {
    fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(HashSet::new())),
            notifier: Arc::new(Notify::new()),
        }
    }

    fn push_assets(&self, addrs: Vec<String>, chain: String, symbol: String) {
        if addrs.is_empty() {
            return;
        }

        let mut buf = self.buffer.lock().unwrap();
        let was_empty = buf.is_empty();
        for addr in addrs {
            buf.insert((addr.as_str(), chain.as_str(), symbol.as_str()).into());
        }
        if was_empty && !buf.is_empty() {
            self.notifier.notify_one();
        }
    }

    async fn wait_and_drain_after_delay(&self, delay_secs: u64) -> Vec<AssetKey> {
        self.notifier.notified().await;
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

        let mut buf = self.buffer.lock().unwrap();
        buf.drain().collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InnerEventHandle {
    inner_event_sender: InnerEventSender,
}

impl InnerEventHandle {
    pub(crate) fn new() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<InnerEvent>();
        let buffer = Arc::new(EventBuffer::new());

        // 接收事件任务
        {
            let buffer = Arc::clone(&buffer);

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    Self::handle_event(event, buffer.clone()).await;
                }
            });
        }

        Self::sync_assets(buffer);

        Self {
            inner_event_sender: tx,
        }
    }

    pub(crate) fn send(&self, event: InnerEvent) -> Result<(), crate::ServiceError> {
        self.inner_event_sender
            .send(event)
            .map_err(|e| crate::SystemError::ChannelSendFailed(e.to_string()))?;
        Ok(())
    }

    async fn handle_event(event: InnerEvent, buffer: Arc<EventBuffer>) {
        match event {
            InnerEvent::SyncAssets {
                addr_list,
                chain_code,
                symbol,
            } => {
                // tracing::info!("开始同步资产");
                buffer.push_assets(addr_list, chain_code, symbol)
            }
        }
    }

    fn sync_assets(buffer: Arc<EventBuffer>) {
        // 批量处理任务
        let buffer = Arc::clone(&buffer);

        tokio::spawn(async move {
            loop {
                let batch = buffer.wait_and_drain_after_delay(5).await;
                tracing::info!("同步资产，batch: {:?}", batch);
                if batch.is_empty() {
                    continue;
                }
                // 分组 chain+symbol → address list
                let mut grouped: HashMap<(String, String), Vec<String>> = HashMap::new();
                for key in batch {
                    grouped
                        .entry((key.chain_code.clone(), key.symbol.clone()))
                        .or_default()
                        .push(key.address.clone());
                }

                // 逐组执行
                for ((chain_code, symbol), addr_list) in grouped {
                    tracing::info!(
                        "Syncing assets: chain={} symbol={} addresses={:?}",
                        chain_code,
                        symbol,
                        addr_list
                    );
                    if let Err(e) = Self::sync_assets_once(chain_code, symbol, addr_list).await {
                        tracing::error!("SyncAssets error: {}", e);
                    }
                }
            }
        });
    }

    async fn sync_assets_once(
        chain_code: String,
        symbol: String,
        addr_list: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        if addr_list.is_empty() {
            return Ok(());
        }

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let repo = wallet_database::factory::RepositoryFactory::repo(pool);

        AssetsService::new(repo)
            .sync_assets_by_addr(addr_list, Some(chain_code), vec![symbol])
            .await
    }
}
