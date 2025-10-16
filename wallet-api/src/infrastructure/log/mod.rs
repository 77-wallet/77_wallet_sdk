pub mod format;
mod offset_tracker;
mod rotator;

use crate::infrastructure::log::format::{CustomEventFormat, LogBasePath};
use offset_tracker::OffsetTracker;
use rotator::SizeRotatingWriter;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncSeekExt as _, BufReader},
    time::interval,
};
use tracing_subscriber::{EnvFilter, Layer, Registry, fmt, layer::SubscriberExt};
use wallet_oss::oss_client;

// 初始化日志。
pub fn init_logger(
    format: CustomEventFormat,
    path: LogBasePath,
    log_level: &str,
) -> Result<(), crate::error::service::ServiceError> {
    let writer = SizeRotatingWriter::new(path.log_path())?;
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);

    let env_filter = EnvFilter::new(log_level);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .event_format(format)
        .with_filter(env_filter.clone());

    // 构建总的 subscriber
    #[cfg(target_os = "android")]
    {
        let android_layer =
            tracing_android::layer("plugin").unwrap().with_filter(env_filter.clone());
        let subscriber = Registry::default().with(android_layer).with(file_layer);

        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set global tracing subscriber");
    }

    #[cfg(target_os = "ios")]
    {
        let subscriber = Registry::default().with(file_layer);
        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set global tracing subscriber");
    }

    #[cfg(all(not(target_os = "android"), not(target_os = "ios")))]
    {
        let stdout_layer = fmt::layer()
            .with_writer(std::io::stdout) // <-- 新增
            .with_ansi(true)
            .with_filter(env_filter);

        let subscriber = Registry::default().with(file_layer).with(stdout_layer);

        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set global tracing subscriber");
    }

    std::mem::forget(guard);
    Ok(())
}

//  上传文件
pub async fn start_upload_scheduler(
    base_path: LogBasePath,
    interval_sec: u64,
    oss_client: Arc<oss_client::OssClient>,
) -> Result<(), crate::error::service::ServiceError> {
    let mut interval = interval(Duration::from_secs(interval_sec));

    tokio::spawn(async move {
        let mut tracker = OffsetTracker::new(base_path.offset_path()).await;
        loop {
            interval.tick().await;

            if let Ok(time) = read_first_line(&base_path.log_path()).await {
                if tracker.get_uid().is_empty() {
                    tracker.set_uid(time.clone());
                }

                if time != tracker.get_uid() {
                    // 将未上报的进行上报
                    if let Err(e) =
                        upload(&base_path.back_file_path(), &mut tracker, &oss_client).await
                    {
                        tracing::error!("upload log to oss error1: {}", e);
                    }

                    // 重置为0
                    tracker.set_offset(0);
                }

                // 上报
                match upload(&base_path.log_path(), &mut tracker, &oss_client).await {
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
    });
    Ok(())
}

async fn read_first_line(path: &PathBuf) -> std::io::Result<String> {
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    Ok(line.trim().to_string())
}

async fn upload(
    path: &Path,
    tracker: &mut OffsetTracker,
    oss_client: &oss_client::OssClient,
) -> Result<u64, crate::error::system::SystemError> {
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
    if let Err(e) = oss_client.upload_buffer(buf, &dst_file_name).await {
        tracing::error!("upload log file error:{}", e);
    };

    // tracing::info!("upload log file success");
    offset += bytes_reader as u64;

    Ok(offset)
}
