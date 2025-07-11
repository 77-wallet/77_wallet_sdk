use crate::{domain::assets::AssetsDomain, FrontendNotifyEvent, NotifyEvent};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;
use tokio_stream::StreamExt as _;
use wallet_database::entities::assets::WalletType;

pub type InnerEventSender = tokio::sync::mpsc::UnboundedSender<InnerEvent>;

pub enum InnerEvent {
    SyncAssets {
        addr_list: Vec<String>,
        chain_code: String,
        symbol: String,
        token_address: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AssetKey {
    address: String,
    chain_code: String,
    symbol: String,
    token_address: Option<String>,
}

impl From<(&str, &str, &str, Option<String>)> for AssetKey {
    fn from(
        (address, chain_code, symbol, token_address): (&str, &str, &str, Option<String>),
    ) -> Self {
        Self {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address,
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

    fn push_assets(
        &self,
        addrs: Vec<String>,
        chain: String,
        symbol: String,
        token_address: Option<String>,
    ) {
        if addrs.is_empty() {
            return;
        }

        let mut buf = self.buffer.lock().unwrap();
        let was_empty = buf.is_empty();
        for addr in addrs {
            buf.insert(
                (
                    addr.as_str(),
                    chain.as_str(),
                    symbol.as_str(),
                    token_address.clone(),
                )
                    .into(),
            );
        }
        if was_empty && !buf.is_empty() {
            self.notifier.notify_one();
        }
    }

    // async fn wait_and_drain_after_delay(&self, delay_secs: u64) -> Vec<AssetKey> {
    //     self.notifier.notified().await;
    //     tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

    //     let mut buf = self.buffer.lock().unwrap();
    //     buf.drain().collect()
    // }

    async fn wait_and_drain_stream(
        &self,
        delay_secs: u64,
    ) -> impl tokio_stream::Stream<Item = Vec<AssetKey>> + '_ {
        use tokio_stream::{wrappers::IntervalStream, StreamExt};

        tracing::debug!("等待第一次资产变更通知...");
        self.notifier.notified().await;
        tracing::debug!("收到资产变更通知，立即执行第一次 drain");
        // 1. 第一次立即 drain
        let first = {
            let mut buf = self.buffer.lock().unwrap();
            let drained = buf.drain().collect::<Vec<_>>();
            tracing::debug!("第一次 drain 获取到 {} 个资产项", drained.len());
            drained
        };

        // 用 stream 返回：第一次立即返回 → 然后每隔 delay 秒返回一次
        let delay = tokio::time::Duration::from_secs(delay_secs);
        let interval = tokio::time::interval(delay);
        let interval_stream = IntervalStream::new(interval).filter_map(move |_| {
            let mut buf = self.buffer.lock().unwrap();
            let drained = buf.drain().collect::<Vec<_>>();
            if drained.is_empty() {
                tracing::debug!("⏳ 定时检查：无新增资产变更，跳过");
                None
            } else {
                // tracing::info!("🔁 定时检查：drain 到 {} 个资产项", drained.len());
                Some(drained)
            }
        });

        tokio_stream::once(first).chain(interval_stream)
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
                token_address,
            } => {
                // tracing::info!("收到资产变更通知，开始同步资产");
                buffer.push_assets(addr_list, chain_code, symbol, token_address)
            }
        }
    }

    fn sync_assets(buffer: Arc<EventBuffer>) {
        // 批量处理任务
        let buffer = Arc::clone(&buffer);

        tokio::spawn(async move {
            let mut stream = buffer.wait_and_drain_stream(5).await;

            while let Some(batch) = stream.next().await {
                tracing::debug!("同步资产，batch: {:?}", batch);
                if batch.is_empty() {
                    continue;
                }
                // 分组 chain+symbol → address list
                let mut grouped: HashMap<(String, String, Option<String>), Vec<String>> =
                    HashMap::new();
                for key in batch {
                    grouped
                        .entry((
                            key.chain_code.clone(),
                            key.symbol.clone(),
                            key.token_address.clone(),
                        ))
                        .or_default()
                        .push(key.address.clone());
                }

                // 逐组执行
                for ((chain_code, symbol, token_address), addr_list) in grouped {
                    tracing::debug!(
                        "Syncing assets: chain={} symbol={} addresses={:?}",
                        chain_code,
                        symbol,
                        addr_list
                    );
                    if let Err(e) =
                        Self::sync_assets_once(chain_code, symbol, addr_list, None).await
                    {
                        tracing::error!("SyncAssets error: {}", e);
                    }
                }
                let data = NotifyEvent::SyncAssets;
                if let Err(e) = FrontendNotifyEvent::new(data).send().await {
                    tracing::error!("SyncAssets send error: {}", e);
                }
            }
        });
    }

    async fn sync_assets_once(
        chain_code: String,
        symbol: String,
        addr_list: Vec<String>,
        wallet_type: Option<WalletType>,
    ) -> Result<(), crate::ServiceError> {
        if addr_list.is_empty() {
            return Ok(());
        }

        AssetsDomain::sync_assets_by_addr_chain(
            addr_list,
            Some(chain_code),
            vec![symbol],
            wallet_type,
        )
        .await
    }
}
