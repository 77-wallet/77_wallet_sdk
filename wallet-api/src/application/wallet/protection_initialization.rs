use super::{WalletApplication, derived_store};
use crate::{
    context::Context,
    device_protection::{ProtectionContext, keystore_format::KeystoreProtectionError},
    error::service::ServiceError,
};
use wallet_database::repositories::config::ConfigRepo;

const KEY: &str = "device_protection.key.v1";
const PENDING: &str = "provisioning-v1";
const READY: &str = "key-ready-v1";

impl WalletApplication {
    /// Explicit first-install initialization, not a migration/recovery shortcut.
    /// Does not enable storage writes. The startup integration must call this
    /// before accepting wallet operations; old storage needs a separate migration.
    pub(crate) async fn initialize_device_protection(
        ctx: &'static Context,
    ) -> Result<(), ServiceError> {
        Self::initialize_device_protection_inner(ctx, None).await
    }

    /// Verify legacy data before first provisioning. READY only checks key usability;
    /// this is not a password-validation API and does not migrate any records.
    pub(crate) async fn initialize_device_protection_for_legacy(
        ctx: &'static Context,
        password: &str,
    ) -> Result<(), ServiceError> {
        Self::initialize_device_protection_inner(
            ctx,
            Some(zeroize::Zeroizing::new(password.to_owned())),
        )
        .await
    }

    async fn initialize_device_protection_inner(
        ctx: &'static Context,
        legacy_password: Option<zeroize::Zeroizing<String>>,
    ) -> Result<(), ServiceError> {
        tokio::spawn(async move {
            let _guard = derived_store::lock().await;
            let api_pool = ctx.api_wallet_pool()?;
            let _api_write_guard =
                api_pool.lock_write_with_metric("device_protection_initialization").await;
            let pool = ctx.core_pool()?;
            let state: Option<(String, i64)> =
                sqlx::query_as("SELECT value, types FROM config WHERE key=?")
                    .bind(KEY)
                    .fetch_optional(pool.read_ref())
                    .await
                    .map_err(|_| {
                        ServiceError::Context("Cannot read device key initialization state".into())
                    })?;
            let ready = match state {
                Some((value, 1)) if value == READY => true,
                Some((value, 1)) if value == PENDING => false,
                None => false,
                _ => return Err(KeystoreProtectionError::InvalidFormat.into()),
            };
            if !ready {
                if let Some(password) = legacy_password.as_deref() {
                    super::legacy_initialization::verify_storage(ctx, password).await?;
                } else {
                    require_empty_storage(ctx).await?;
                }
                ConfigRepo::upsert(KEY, PENDING, Some(1), &pool).await?;
                ctx.device_protection()
                    .provision_for_new_storage()
                    .await
                    .map_err(KeystoreProtectionError::from)?;
            }
            // READY retries deliberately never provision, including after KeyMissing.
            // This non-secret challenge checks usability; it is not attestation and
            // does not prove that any existing wallet ciphertext can be recovered.
            use rand::RngCore;
            let mut challenge = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut challenge);
            let binding = ProtectionContext::new("device-protection", "initialization-probe-v1")
                .map_err(KeystoreProtectionError::from)?;
            let blob = ctx
                .device_protection()
                .protect(&binding, &challenge)
                .await
                .map_err(KeystoreProtectionError::from)?;
            let opened = ctx
                .device_protection()
                .unprotect(&binding, &blob)
                .await
                .map_err(KeystoreProtectionError::from)?;
            if opened != challenge {
                return Err(KeystoreProtectionError::InvalidFormat.into());
            }
            if !ready {
                ConfigRepo::upsert(KEY, READY, Some(1), &pool).await?;
            }
            Ok::<_, ServiceError>(())
        })
        .await
        .map_err(|_| KeystoreProtectionError::Worker)?
    }
}

async fn require_empty_storage(ctx: &Context) -> Result<(), ServiceError> {
    let core = ctx.core_pool()?;
    let occupied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM wallet) OR EXISTS(SELECT 1 FROM account) OR EXISTS(SELECT 1 FROM device WHERE password_proof IS NOT NULL) OR EXISTS(SELECT 1 FROM config WHERE key LIKE 'device_protection.migration.v1:%')",
    ).fetch_one(core.read_ref()).await
        .map_err(|_| ServiceError::Context("Cannot inspect wallet initialization state".into()))?;
    let api = ctx.api_wallet_pool()?;
    let api_occupied: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM api_wallet) OR EXISTS(SELECT 1 FROM api_account)",
    )
    .fetch_one(api.read_ref())
    .await
    .map_err(|_| ServiceError::Context("Cannot inspect API wallet initialization state".into()))?;
    if occupied || api_occupied {
        return Err(KeystoreProtectionError::LegacyForbidden.into());
    }
    let dirs = ctx.get_global_dirs().clone();
    tokio::task::spawn_blocking(move || -> Result<(), ServiceError> {
        use wallet_tree::file_ops::inventory::{scan_verify, scan_wallet};
        if scan_verify(&dirs.root_dir)?.is_some() {
            return Err(KeystoreProtectionError::LegacyForbidden.into());
        }
        for entry in
            std::fs::read_dir(&dirs.wallet_dir).map_err(|_| KeystoreProtectionError::Storage)?
        {
            let entry = entry.map_err(|_| KeystoreProtectionError::Storage)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| KeystoreProtectionError::InvalidFormat)?;
            if !scan_wallet(&entry.path(), &name)?.is_empty() {
                return Err(KeystoreProtectionError::LegacyForbidden.into());
            }
            // A stale public index is evidence of previous accounts, even if
            // every private-key file has disappeared. Do not infer fresh storage.
            if entry
                .path()
                .join("subs/derived_meta.json")
                .try_exists()
                .map_err(|_| KeystoreProtectionError::Storage)?
            {
                return Err(KeystoreProtectionError::LegacyForbidden.into());
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| KeystoreProtectionError::Worker)?
}
