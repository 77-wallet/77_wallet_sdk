use crate::{
    domain::{api_wallet::assets::ApiAssetsDomain, assets::AssetsDomain},
    error::service::ServiceError,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;
use tokio_stream::StreamExt as _;

pub(crate) type InnerEventSender = tokio::sync::mpsc::UnboundedSender<InnerEvent>;

pub(crate) struct SyncAssetsData {
    // pub(crate) uid: String,
    pub(crate) addr_list: Vec<String>,
    pub(crate) chain_code: String,
    pub(crate) symbol: Vec<String>,
    pub(crate) token_address: Option<String>,
}

impl SyncAssetsData {
    pub(crate) fn new(
        // uid: String,
        addr_list: Vec<String>,
        chain_code: String,
        symbol: Vec<String>,
        token_address: Option<String>,
    ) -> Self {
        Self { addr_list, chain_code, symbol, token_address }
    }
}

pub(crate) enum InnerEvent {
    SyncAssets(SyncAssetsData),
    ApiWalletSyncAssets(SyncAssetsData),
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
        Self { buffer: Arc::new(Mutex::new(HashSet::new())), notifier: Arc::new(Notify::new()) }
    }

    fn push_assets(&self, data: SyncAssetsData) {
        if data.addr_list.is_empty() {
            return;
        }

        let mut buf = self.buffer.lock().unwrap();
        let was_empty = buf.is_empty();
        for addr in data.addr_list {
            for s in &data.symbol {
                buf.insert(AssetKey::from((addr.as_str(), data.chain_code.as_str(), s.as_str())));
            }
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
        use tokio_stream::{StreamExt, wrappers::IntervalStream};

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
        let normal_buffer = Arc::new(EventBuffer::new());
        let api_buffer = Arc::new(EventBuffer::new());
        // 接收事件任务
        {
            let normal_buf = Arc::clone(&normal_buffer);
            let api_buf = Arc::clone(&api_buffer);

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        InnerEvent::SyncAssets(data) => {
                            normal_buf.push_assets(data);
                        }
                        InnerEvent::ApiWalletSyncAssets(data) => {
                            api_buf.push_assets(data);
                        }
                    }
                }
            });
        }

        Self::start_sync_loop(Arc::clone(&normal_buffer), SyncTarget::Assets);
        Self::start_sync_loop(Arc::clone(&api_buffer), SyncTarget::ApiAssets);

        Self { inner_event_sender: tx }
    }

    pub(crate) fn send(&self, event: InnerEvent) -> Result<(), ServiceError> {
        self.inner_event_sender
            .send(event)
            .map_err(|e| crate::error::system::SystemError::ChannelSendFailed(e.to_string()))?;
        Ok(())
    }

    fn start_sync_loop(buffer: Arc<EventBuffer>, target: SyncTarget) {
        tokio::spawn(async move {
            let mut stream = buffer.wait_and_drain_stream(5).await;

            while let Some(batch) = stream.next().await {
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

                for ((chain_code, symbol), addr_list) in grouped {
                    if let Err(e) =
                        Self::sync_assets_once(chain_code, symbol, addr_list, target.clone()).await
                    {
                        tracing::error!("{:?} sync error: {}", target, e);
                    }
                }
                let notify_type = match target {
                    SyncTarget::Assets => NotifyEvent::SyncAssets,
                    SyncTarget::ApiAssets => NotifyEvent::ApiWalletSyncAssets,
                };
                if let Err(e) = FrontendNotifyEvent::new(notify_type).send().await {
                    tracing::error!("{:?} send error: {}", target, e);
                }
            }
        });
    }

    async fn sync_assets_once(
        chain_code: String,
        symbol: String,
        addr_list: Vec<String>,
        target: SyncTarget,
    ) -> Result<(), crate::error::service::ServiceError> {
        if addr_list.is_empty() {
            return Ok(());
        }

        match target {
            SyncTarget::Assets => {
                AssetsDomain::sync_assets_by_addr_chain(addr_list, Some(chain_code), vec![symbol])
                    .await
            }
            SyncTarget::ApiAssets => {
                tracing::info!("sync assets by addr chain: {:?}", addr_list);
                ApiAssetsDomain::sync_assets_by_addr_chain(
                    addr_list,
                    Some(chain_code),
                    vec![symbol],
                )
                .await
            }
        }
    }
}

#[derive(Debug, Clone)]
enum SyncTarget {
    Assets,
    ApiAssets,
}
