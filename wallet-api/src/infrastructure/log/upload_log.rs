use crate::{
    error::service::ServiceError,
    infrastructure::log::{format::LogBasePath, offset_tracker::OffsetTracker},
};
use std::{io::SeekFrom, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader},
    sync::{Mutex, broadcast},
    task::JoinHandle,
    time::interval,
};
use wallet_transport_backend::log_upload::LogUploadClient;

#[async_trait::async_trait]
trait LogUploader: Send + Sync {
    async fn upload(
        &self,
        sn: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<(), crate::error::system::SystemError>;
}

#[async_trait::async_trait]
impl LogUploader for LogUploadClient {
    async fn upload(
        &self,
        sn: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<(), crate::error::system::SystemError> {
        LogUploadClient::upload(self, sn, filename, data)
            .await
            .map_err(|e| crate::error::system::SystemError::Service(e.to_string()))
    }
}

#[derive(Debug)]
pub struct UploadLogHandle {
    shutdown_tx: broadcast::Sender<()>,
    handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
}

impl UploadLogHandle {
    pub async fn new(
        base_path: LogBasePath,
        interval_sec: u64,
        client: Option<Arc<LogUploadClient>>,
        sn: String,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let handle = client.map(|client| {
            let mut worker = UploadLogConsumer::new(
                shutdown_tx.subscribe(),
                base_path,
                interval_sec,
                client,
                sn,
            );
            tokio::spawn(async move { worker.run().await })
        });
        Self { shutdown_tx, handle: Mutex::new(handle) }
    }

    pub(crate) async fn close(&self) -> Result<(), ServiceError> {
        let _ = self.shutdown_tx.send(());
        if let Some(handle) = self.handle.lock().await.take() {
            handle.await.map_err(|_| {
                ServiceError::System(crate::error::system::SystemError::BackendEndpointNotFound)
            })??;
        }
        Ok(())
    }
}

struct UploadLogConsumer {
    shutdown_rx: broadcast::Receiver<()>,
    base_path: LogBasePath,
    interval_sec: u64,
    uploader: Arc<dyn LogUploader>,
    sn: String,
}

impl UploadLogConsumer {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        base_path: LogBasePath,
        interval_sec: u64,
        uploader: Arc<dyn LogUploader>,
        sn: String,
    ) -> Self {
        Self { shutdown_rx, base_path, interval_sec, uploader, sn }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        let mut interval = interval(Duration::from_secs(self.interval_sec));
        let mut tracker = OffsetTracker::new(self.base_path.offset_path()).await;
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => break,
                _ = interval.tick() => {
                    if let Err(e) = self.upload_pending(&mut tracker).await {
                        tracing::error!("backend log upload failed: {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    async fn upload_pending(
        &self,
        tracker: &mut OffsetTracker,
    ) -> Result<(), crate::error::system::SystemError> {
        // Read identity and content through the same descriptor, even if the path rotates.
        let file = File::open(self.base_path.log_path()).await?;
        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        let header_bytes = reader.read_line(&mut first_line).await? as u64;
        let uid = first_line.trim();
        let offset = if uid == tracker.get_uid() {
            tracker.get_offset().max(header_bytes)
        } else {
            header_bytes
        };
        reader.seek(SeekFrom::Start(offset)).await?;
        let mut data = Vec::new();
        let bytes_read = reader.read_to_end(&mut data).await?;
        if data.len() < 1024 {
            return Ok(());
        }
        let filename = format!("sdk:{}.txt", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));
        self.uploader.upload(&self.sn, &filename, data).await?;
        // A rejected request must leave BOTH offset and identity unchanged for retry.
        tracker.set_uid(uid.to_owned());
        tracker.set_offset(offset + bytes_read as u64);
        tracker.save().await;
        tracing::info!(bytes = bytes_read, "backend log upload succeeded");
        Ok(())
    }
}

#[cfg(test)]
#[path = "upload_log_tests.rs"]
mod tests;
