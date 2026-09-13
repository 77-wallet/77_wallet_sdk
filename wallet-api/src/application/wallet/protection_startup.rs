//! Activate ordinary-file protection at existing SDK password entry points.
use super::{
    WalletApplication, derived_store, legacy_initialization, protection_initialization,
    protection_rollout,
};
use crate::{
    context::Context,
    device_protection::{ProtectionError, keystore_format::KeystoreProtectionError},
    domain::app::config::ConfigDomain,
    error::service::ServiceError,
};
use std::sync::LazyLock;

// Covers first-password validation, format upgrade and activation together. The
// file gate still protects individual storage operations and their owned tasks.
static SETUP: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

impl WalletApplication {
    /// Called before manager workers. Never provisions without a real password.
    pub(crate) async fn prepare_device_protection_startup(
        ctx: &'static Context,
    ) -> Result<(), ServiceError> {
        let active = protection_rollout::files_active(ctx).await?;
        let state = protection_initialization::key_state(ctx).await?;
        if state == Some(protection_initialization::KeyState::Ready) {
            Self::initialize_device_protection(ctx).await?;
        } else if active {
            return Err(KeystoreProtectionError::LegacyForbidden.into());
        }
        Ok(())
    }

    pub(crate) async fn validate_password(
        ctx: &'static Context,
        password: &str,
    ) -> Result<(), ServiceError> {
        let password = zeroize::Zeroizing::new(password.to_owned());
        tokio::spawn(async move {
            let _setup = SETUP.lock().await;
            let active = protection_rollout::files_active(ctx).await?;
            let key_state = protection_initialization::key_state(ctx).await?;
            let requested =
                ctx.device_protection().is_configured() || active || key_state.is_some();
            if requested && !ctx.device_protection().is_configured() {
                return Err(
                    KeystoreProtectionError::Protection(ProtectionError::NotConfigured).into()
                );
            }
            // A session token is not an encryption password. Use this Context's
            // session, never the unrelated process-global runtime in another Context.
            let token = ctx
                .wallet_unlock_session_snapshot()
                .await
                .is_some_and(|session| session.session_token() == password.as_str());
            if token {
                if requested && !active {
                    return Err(crate::error::system::SystemError::SystemNotReady.into());
                }
                if active {
                    Self::initialize_device_protection(ctx).await?;
                }
                return Ok(());
            }
            Self::validate_password_inner(ctx, &password).await?;
            if !requested {
                return Ok(());
            }
            if active {
                // Avoid decrypting/migrating the whole inventory on every login.
                Self::initialize_device_protection(ctx).await?;
                return Ok(());
            }
            prepare_format(ctx, &password).await?;
            Self::migrate_wallet_file_protection(ctx, &password).await?;
            Ok(())
        })
        .await
        .map_err(|_| KeystoreProtectionError::Worker)?
    }
}

async fn prepare_format(ctx: &'static Context, password: &str) -> Result<(), ServiceError> {
    if matches!(
        ConfigDomain::get_wallet_tree_strategy(ctx).await?,
        wallet_tree::WalletTreeStrategy::V2
    ) {
        return Ok(());
    }
    {
        let _guard = derived_store::lock().await;
        let api = ctx.api_wallet_pool()?;
        let _api_guard = api.lock_write_with_metric("device_protection_empty_format").await;
        if legacy_initialization::snapshot(ctx).await?.is_empty() {
            // Fresh ordinary storage has nothing to convert. Verify any API/proof
            // data first, then select V2 without creating an unnecessary verify file.
            protection_initialization::initialize_under_gate(ctx, Some(password)).await?;
            ConfigDomain::set_config(
                ctx,
                wallet_database::entities::config::config_key::WALLET_TREE_STRATEGY,
                &wallet_database::entities::config::WalletTreeStrategy {
                    wallet_tree_strategy: wallet_tree::WalletTreeStrategy::V2,
                }
                .to_json_str()?,
            )
            .await?;
            return Ok(());
        }
    }
    WalletApplication::upgrade_algorithm(ctx, password).await
}
