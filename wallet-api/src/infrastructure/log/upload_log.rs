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
use wallet_oss::oss_client;

#[derive(Debug)]
pub struct UploadLogHandle {
    shutdown_tx: broadcast::Sender<()>,
    handle: Mutex<Option<JoinHandle<Result<(), ServiceError>>>>,
}

impl UploadLogHandle {
    pub async fn new(
        base_path: LogBasePath,
        interval_sec: u64,
        oss_client: Arc<oss_client::OssClient>,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_rx1 = shutdown_tx.subscribe();
        // 发交易
        let mut tx = UploadLogConsumer::new(shutdown_rx1, base_path, interval_sec, oss_client);
        let tx_handle = tokio::spawn(async move { tx.run().await });
        Self { shutdown_tx, handle: Mutex::new(Some(tx_handle)) }
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
    oss_client: Arc<oss_client::OssClient>,
}

impl UploadLogConsumer {
    fn new(
        shutdown_rx: broadcast::Receiver<()>,
        base_path: LogBasePath,
        interval_sec: u64,
        oss_client: Arc<oss_client::OssClient>,
    ) -> Self {
        Self { shutdown_rx, base_path, interval_sec, oss_client }
    }

    async fn run(&mut self) -> Result<(), ServiceError> {
        tracing::info!("starting process upload log -------------------------------");
        let mut interval = interval(Duration::from_secs(self.interval_sec));
        let mut tracker = OffsetTracker::new(self.base_path.offset_path()).await;
        loop {
            tokio::select! {
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("closing process upload log -------------------------------");
                    break;
                }
                _ = interval.tick() => {
                     if let Ok(time) = self.read_first_line().await {
                        if tracker.get_uid().is_empty() {
                            tracker.set_uid(time.clone());
                        }

                        if time != tracker.get_uid() {
                            // 将未上报的进行上报
                            if let Err(e) = self
                                .upload(&mut tracker)
                                .await
                            {
                                tracing::error!("upload log to oss error1: {}", e);
                            }

                            // 重置为0
                            tracker.set_offset(0);
                        }

                        // 上报
                        match self.upload(&mut tracker).await
                        {
                            Ok(new_offset) => {
                                tracker.set_offset(new_offset);
                                tracker.save().await;
                            }
                            Err(e) => {
                                tracing::error!("upload log to oss error: {}", e);
                            }
                        }
                    }
                }
            }
        }
        tracing::info!("closing process upload log ------------------------------- end");
        Ok(())
    }

    async fn read_first_line(&self) -> std::io::Result<String> {
        let path = &self.base_path.log_path();
        let file = File::open(path).await?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        Ok(line.trim().to_string())
    }

    async fn upload(
        &self,
        tracker: &mut OffsetTracker,
    ) -> Result<u64, crate::error::system::SystemError> {
        let path = &self.base_path.log_path();
        let file = File::open(path).await?;
        let mut reader = BufReader::new(file);
        let mut offset = tracker.get_offset();

        if offset == 0 {
            let mut first_line = String::new();
            let bytes = reader.read_line(&mut first_line).await?;
            offset += bytes as u64;
        }

        reader.seek(SeekFrom::Start(offset)).await?;

        let mut buf = Vec::new();
        let bytes_reader = reader.read_to_end(&mut buf).await?;

        // 数据太少了,下次上报
        if buf.len() < 1024 {
            return Ok(offset);
        }

        // println!("content");
        // println!("{}", String::from_utf8_lossy(&buf));

        // 上传文件
        let timestamp = chrono::Utc::now();
        let dst_file_name = format!("sdk:{}.txt", timestamp.format("%Y-%m-%d %H:%M:%S"));
        if let Err(e) = self.oss_client.upload_buffer(buf, &dst_file_name).await {
            tracing::error!("upload log file error:{}", e);
        };

        // tracing::info!("upload log file success");
        offset += bytes_reader as u64;

        Ok(offset)
    }
}
