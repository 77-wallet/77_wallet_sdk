use crate::manager::Context;
use std::time::Duration;
use tokio::time;

pub async fn periodic_log_report(interval: Duration) {
    tokio::spawn(async move {
        let mut interval = time::interval(interval);
        // let pool = crate::manager::Context::get_global_sqlite_pool().unwrap();
        loop {
            interval.tick().await;
            // let start = Instant::now();

            // Create a new AppService instance for each iteration
            // let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
            // let app_service = AppService::new(repo);

            // Perform log reporting here
            let res = upload_log_file().await;
            tracing::info!("upload_log_file: res: {res:?}");
            // let elapsed = start.elapsed();
        }
    });
}

pub async fn upload_log_file() -> Result<(), crate::ServiceError> {
    let oss_client = crate::manager::Context::get_global_oss_client()?;
    let dirs = Context::get_global_dirs()?;
    let log_dir = &dirs.log_dir.to_string_lossy().to_string();
    let level = wallet_utils::log::get_log_level();
    let src_file_path = format!("{log_dir}/{level}.txt");

    if wallet_utils::file_func::is_file_empty(&src_file_path)? {
        return Ok(());
    };

    let timestamp = sqlx::types::chrono::Utc::now();
    let dst_file_name = format!("sdk:{}.txt", timestamp.format("%Y-%m-%d %H:%M:%S"));
    tracing::info!(
        "upload_log_file: src_file_path: {src_file_path}, dst_file_name: {dst_file_name}"
    );
    oss_client
        .upload_local_file(&src_file_path, &dst_file_name)
        .await?;

    // Clear the log file after successful upload
    wallet_utils::file_func::clear_file(&src_file_path)?;
    tracing::info!(
        "Log file cleared after successful upload: {}",
        src_file_path
    );

    Ok(())
}
// Example usage:
// tokio::spawn(periodic_log_report(Duration::from_secs(300)));

#[cfg(test)]
mod tests {

    #[test]
    fn test_generate_dst_file_name() {
        let timestamp = sqlx::types::chrono::Utc::now();
        let dst_file_name = format!("sdk:{}.txt", timestamp.format("%Y-%m-%d %H:%M:%S"));

        // Assert the expected format
        assert_eq!(dst_file_name, "sdk:2023-05-15 10:30:00 UTC.txt");
    }
}
