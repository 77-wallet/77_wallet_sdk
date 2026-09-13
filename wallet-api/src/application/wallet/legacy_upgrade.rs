use crate::{
    context::Context,
    device_protection::keystore_format::{KeystoreProtectionError, has_device_header},
    error::service::ServiceError,
};

/// V1-to-V2 conversion must finish before device protection starts. Checking only
/// extant file headers would miss a deleted protected file with durable progress.
pub(super) async fn preflight(ctx: &Context) -> Result<(), ServiceError> {
    let pool = ctx.core_pool()?;
    let started: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM config WHERE key LIKE ?)")
        .bind("device_protection.migration.v1:%")
        .fetch_one(pool.read_ref())
        .await
        .map_err(|_| ServiceError::Context("Cannot read keystore migration progress".into()))?;
    if started {
        return Err(KeystoreProtectionError::LegacyForbidden.into());
    }

    let dirs = ctx.get_global_dirs().clone();
    let paths = tokio::task::spawn_blocking(
        move || -> Result<Vec<std::path::PathBuf>, wallet_tree::Error> {
            use wallet_tree::file_ops::inventory::{scan_verify, scan_wallet};
            let mut paths = Vec::new();
            // Reject unknown entries/symlinks rather than overlooking protected data.
            for entry in std::fs::read_dir(&dirs.wallet_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let address = name.to_str().ok_or(wallet_tree::Error::FilenameInvalid)?;
                paths.extend(
                    scan_wallet(&entry.path(), address)?.into_iter().map(|record| record.path),
                );
            }
            if let Some(verify) = scan_verify(&dirs.root_dir)? {
                paths.push(verify.path);
            }
            Ok(paths)
        },
    )
    .await
    .map_err(|_| KeystoreProtectionError::Worker)??;
    for path in paths {
        if has_device_header(&path).await? {
            return Err(KeystoreProtectionError::LegacyForbidden.into());
        }
    }
    Ok(())
}
