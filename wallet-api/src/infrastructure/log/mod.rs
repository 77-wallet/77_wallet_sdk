use offset_tracker::OffsetTracker;
use rotator::SizeRotatingWriter;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncSeekExt as _, BufReader},
    time::interval,
};
use tracing_subscriber::EnvFilter;
use wallet_oss::oss_client;

mod format;
pub use format::*;
mod offset_tracker;
mod rotator;

// 初始化日志。
pub fn init_logger(
    format: CustomEventFormat,
    path: LogBasePath,
    log_level: &str,
) -> Result<(), crate::ServiceError> {
    let max_size = 1024 * 1024 * 5;
    let max_files = 3;

    let writer = SizeRotatingWriter::new(path.log_path(), max_size, max_files)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);

    let env_filter = EnvFilter::new(log_level);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(env_filter)
        .event_format(format)
        .init();

    std::mem::forget(guard);
    Ok(())
}

//  上传文件
pub async fn start_upload_scheduler(
    base_path: LogBasePath,
    interval_sec: u64,
    oss_client: oss_client::OssClient,
) -> Result<(), crate::ServiceError> {
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
                    let _ = upload(&base_path.back_file_path(), &mut tracker, &oss_client)
                        .await
                        .unwrap();

                    // 重置为0
                    tracker.set_offset(0);

                    // 重置为0
                    tracker.set_offset(0);
                }

                // 上报
                let new_offset = upload(&base_path.log_path(), &mut tracker, &oss_client)
                    .await
                    .unwrap();
                tracker.set_offset(new_offset);
                tracker.save().await;
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
) -> Result<u64, crate::SystemError> {
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

    if buf.len() < 1024 {
        return Ok(offset);
    }

    // println!("content");
    // println!("{}", String::from_utf8_lossy(&buf));

    // // 上传文件
    let timestamp = chrono::Utc::now();
    let dst_file_name = format!("sdk:{}.txt", timestamp.format("%Y-%m-%d %H:%M:%S"));
    if let Err(e) = oss_client.upload_buffer(buf, &dst_file_name).await {
        tracing::error!("upload log file error:{}", e);
    };

    offset += bytes_reader as u64;

    Ok(offset)
}
