use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    context::{Context, get_context},
    domain::api_wallet::unlock::{
        WalletUnlockMaterial, WalletUnlockSession, WalletUnlockSessionCodec,
    },
    error::service::ServiceError,
};
use once_cell::sync::OnceCell;

struct UnlockSessionRuntime {
    context: &'static Context,
    cleanup_started: AtomicBool,
}

static RUNTIME: OnceCell<UnlockSessionRuntime> = OnceCell::new();

fn ensure_runtime() -> Result<&'static UnlockSessionRuntime, ServiceError> {
    let context = get_context()?;
    if let Some(runtime) = RUNTIME.get() {
        if std::ptr::eq(runtime.context, context) {
            return Ok(runtime);
        }

        return Err(crate::error::service::ServiceError::System(
            crate::error::system::SystemError::Internal(
                "unlock session runtime already initialized with another context".to_string(),
            ),
        ));
    }

    RUNTIME
        .set(UnlockSessionRuntime { context, cleanup_started: AtomicBool::new(false) })
        .map_err(|_| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(
                    "unlock session runtime initialization failed".to_string(),
                ),
            )
        })?;

    runtime()
}

fn runtime() -> Result<&'static UnlockSessionRuntime, ServiceError> {
    RUNTIME.get().ok_or_else(|| crate::error::system::SystemError::ContextNotInit.into())
}

fn context() -> Result<&'static Context, ServiceError> {
    Ok(runtime()?.context)
}

pub(crate) async fn cleanup_wallet_unlock_session_if_expired() -> Result<bool, ServiceError> {
    let context = context()?;
    let Some(session) = context.wallet_unlock_session_snapshot().await else {
        return Ok(false);
    };

    if !session.is_expired() {
        return Ok(false);
    }

    tracing::info!("wallet unlock session expired, clearing session");
    context.clear_wallet_unlock_session().await?;
    Ok(true)
}

pub(crate) async fn start_wallet_unlock_session_cleanup_task() -> Result<(), ServiceError> {
    let runtime = ensure_runtime()?;
    if runtime
        .cleanup_started
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }

    tokio::spawn(async move {
        let mut interval = crate::infrastructure::runtime::time::new_production_interval(
            WalletUnlockSessionCodec::unlock_session_cleanup_interval(),
        );
        loop {
            interval.tick().await;
            match cleanup_wallet_unlock_session_if_expired().await {
                Ok(true) => {
                    tracing::info!("wallet unlock session cleanup loop removed expired session");
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::error!("wallet unlock session cleanup loop failed: {:?}", err);
                }
            }
        }
    });

    Ok(())
}

pub(crate) async fn wallet_unlock_token() -> Result<String, ServiceError> {
    let _ = cleanup_wallet_unlock_session_if_expired().await?;
    let context = context()?;
    let Some(session) = context.wallet_unlock_session_snapshot().await else {
        return Err(crate::error::system::SystemError::SystemNotReady.into());
    };

    if session.is_expired() {
        context.clear_wallet_unlock_session().await?;
        return Err(crate::error::system::SystemError::SystemNotReady.into());
    }

    Ok(session.session_token().to_string())
}

pub(crate) async fn wallet_unlock_token_is_active(token: &str) -> Result<bool, ServiceError> {
    let _ = cleanup_wallet_unlock_session_if_expired().await?;
    let context = context()?;
    let Some(session) = context.wallet_unlock_session_snapshot().await else {
        return Ok(false);
    };

    if session.is_expired() {
        context.clear_wallet_unlock_session().await?;
        return Ok(false);
    }

    Ok(session.session_token() == token)
}

pub(crate) async fn wallet_unlock_material(
    wallet_address: &str,
) -> Result<WalletUnlockMaterial, ServiceError> {
    let _ = cleanup_wallet_unlock_session_if_expired().await?;
    let context = context()?;
    let Some(session) = context.wallet_unlock_session_snapshot().await else {
        return Err(crate::error::system::SystemError::SystemNotReady.into());
    };

    if session.is_expired() {
        context.clear_wallet_unlock_session().await?;
        return Err(crate::error::system::SystemError::SystemNotReady.into());
    }

    session
        .wallet_material(wallet_address)
        .cloned()
        .ok_or_else(|| crate::error::system::SystemError::SystemNotReady.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApiWalletBackend,
        context::{get_context, init_context_with_api_wallet_backend},
        dirs::Dirs,
    };
    use async_trait::async_trait;
    use std::{
        collections::HashMap,
        sync::Arc,
        time::{Duration, Instant},
    };
    use tempfile::TempDir;
    use wallet_transport_backend::{
        request::{
            KeysInitReq,
            api_wallet::wallet::{
                AppIdImportRechargeWalletReq, AppIdImportReq, AppIdUidUsageReq, BindAppIdReq,
            },
        },
        response_vo::api_wallet::wallet::{
            AppIdUidUsageRes, KeysUidCheckRes, QueryUidBindInfoRes, UidStatus,
        },
    };

    const TEST_SN: &str = "context-unlock-session-sn";
    const TEST_DEVICE_TYPE: &str = "ANDROID";

    #[derive(Clone)]
    struct UnlockSessionTestEnv {
        _tempdir: Arc<TempDir>,
    }

    #[derive(Default)]
    struct NoopApiWalletBackend;

    #[async_trait]
    impl ApiWalletBackend for NoopApiWalletBackend {
        async fn wallet_bind_appid(&self, _: BindAppIdReq) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn init_api_wallet(&self, _: AppIdImportReq) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn old_keys_init(&self, _: KeysInitReq) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn appid_import(&self, _: AppIdImportReq) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn appid_import_recharge_wallet(
            &self,
            _: AppIdImportRechargeWalletReq,
        ) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn keys_uid_check(&self, uid: &str) -> Result<KeysUidCheckRes, ServiceError> {
            Ok(KeysUidCheckRes { uid: uid.to_string(), status: UidStatus::ApiRaw })
        }

        async fn query_uid_bind_info(
            &self,
            uid: &str,
        ) -> Result<QueryUidBindInfoRes, ServiceError> {
            Ok(QueryUidBindInfoRes {
                app_id: String::new(),
                org_id: String::new(),
                bind_status: false,
                sn: uid.to_string(),
            })
        }

        async fn appid_uid_usage(
            &self,
            _: AppIdUidUsageReq,
        ) -> Result<AppIdUidUsageRes, ServiceError> {
            Ok(AppIdUidUsageRes { used: false })
        }
    }

    static TEST_ENV: once_cell::sync::Lazy<tokio::sync::OnceCell<UnlockSessionTestEnv>> =
        once_cell::sync::Lazy::new(tokio::sync::OnceCell::new);

    async fn unlock_session_env() -> &'static UnlockSessionTestEnv {
        TEST_ENV
            .get_or_init(|| async {
                unsafe {
                    std::env::set_var("WALLET_TRANSPORT_NO_PROXY", "1");
                }

                let config = crate::config::Config::new(
                    r#"
app_code: "test"
crypto:
  aes_key: "1234567890abcdef"
  aes_iv: "abcdef1234567890"
backend_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
aggregate_api:
  dev_url: "http://127.0.0.1:9"
  test_url: "http://127.0.0.1:9"
  prod_url: "http://127.0.0.1:9"
oss:
  access_key_id: "id"
  access_key_secret: "secret"
  bucket_name: "bucket"
  endpoint: "oss-endpoint"
"#,
                )
                .expect("parse test config");

                let tempdir = TempDir::new().expect("create tempdir");
                let dirs = Dirs::new(tempdir.path().to_str().expect("utf8 root dir"))
                    .expect("create dirs");
                init_context_with_api_wallet_backend(
                    TEST_SN,
                    TEST_DEVICE_TYPE,
                    dirs,
                    None,
                    config,
                    Arc::new(NoopApiWalletBackend::default()),
                )
                .await
                .expect("init test context");

                UnlockSessionTestEnv { _tempdir: Arc::new(tempdir) }
            })
            .await
    }

    #[tokio::test]
    async fn wallet_unlock_session_flow_logs() {
        let _env = unlock_session_env().await;
        let context = get_context().expect("context");
        start_wallet_unlock_session_cleanup_task().await.expect("init runtime");

        let wallet_address = "0xcontext-unlock-session";
        let unlock_material = WalletUnlockMaterial::new(vec![0x11; 32]);
        let mut wallet_materials = HashMap::new();
        wallet_materials.insert(wallet_address.to_string(), unlock_material.clone());

        eprintln!("[context-unlock] 1) prepare unlock material and session");
        let session_token = WalletUnlockSessionCodec::generate_unlock_token();
        let unlock_session = WalletUnlockSession::new(
            session_token.clone(),
            Instant::now() + Duration::from_secs(60),
            wallet_materials,
        );
        eprintln!(
            "[context-unlock] 1.1) session ready (token_len={}, wallet_material_count={})",
            session_token.len(),
            1
        );

        eprintln!("[context-unlock] 2) store unlock session in context");
        context.set_wallet_unlock_session(unlock_session).await.expect("store unlock session");

        eprintln!("[context-unlock] 3) read token from context");
        let stored_token = wallet_unlock_token().await.expect("read token");
        eprintln!(
            "[context-unlock] 3.1) token roundtrip ok (token_len={}, active={})",
            stored_token.len(),
            wallet_unlock_token_is_active(&stored_token).await.expect("check token active")
        );
        assert_eq!(stored_token, session_token);

        eprintln!("[context-unlock] 4) read wallet material from context");
        let stored_material =
            wallet_unlock_material(wallet_address).await.expect("read wallet material");
        eprintln!(
            "[context-unlock] 4.1) wallet material ready (smk_len={}, matches_expected={})",
            stored_material.smk().len(),
            stored_material.smk() == unlock_material.smk()
        );
        assert_eq!(stored_material.smk(), unlock_material.smk());

        eprintln!("[context-unlock] 5) clear unlock session");
        context.clear_wallet_unlock_session().await.expect("clear unlock session");
        eprintln!("[context-unlock] 5.1) session cleared");

        let token_err = wallet_unlock_token().await.expect_err("token should be gone");
        eprintln!("[context-unlock] 5.2) token read after clear errored: {token_err:?}");
        assert!(matches!(
            token_err,
            ServiceError::System(crate::error::system::SystemError::SystemNotReady)
        ));

        let material_err =
            wallet_unlock_material(wallet_address).await.expect_err("material should be gone");
        eprintln!("[context-unlock] 5.3) material read after clear errored: {material_err:?}");
        assert!(matches!(
            material_err,
            ServiceError::System(crate::error::system::SystemError::SystemNotReady)
        ));
    }

    #[tokio::test]
    async fn wallet_unlock_session_cleanup_logs() {
        let _env = unlock_session_env().await;
        let context = get_context().expect("context");
        start_wallet_unlock_session_cleanup_task().await.expect("init runtime");

        let wallet_address = "0xcontext-unlock-session-expired";
        let unlock_material = WalletUnlockMaterial::new(vec![0x22; 32]);
        let mut wallet_materials = HashMap::new();
        wallet_materials.insert(wallet_address.to_string(), unlock_material.clone());

        let unlock_session = WalletUnlockSession::new(
            "expired-unlock-token".to_string(),
            Instant::now() - Duration::from_millis(1),
            wallet_materials,
        );
        eprintln!("[context-unlock] cleanup 1) store already-expired unlock session");
        context
            .set_wallet_unlock_session(unlock_session)
            .await
            .expect("store expired unlock session");

        eprintln!("[context-unlock] cleanup 2) run cleanup helper");
        let cleaned =
            cleanup_wallet_unlock_session_if_expired().await.expect("cleanup expired session");
        eprintln!("[context-unlock] cleanup 2.1) cleanup returned {cleaned}");
        assert!(cleaned);

        eprintln!("[context-unlock] cleanup 3) verify token/material are gone");
        let token_err =
            wallet_unlock_token().await.expect_err("token should be gone after cleanup");
        eprintln!("[context-unlock] cleanup 3.1) token read errored: {token_err:?}");
        assert!(matches!(
            token_err,
            ServiceError::System(crate::error::system::SystemError::SystemNotReady)
        ));

        let material_err = wallet_unlock_material(wallet_address)
            .await
            .expect_err("material should be gone after cleanup");
        eprintln!("[context-unlock] cleanup 3.2) material read errored: {material_err:?}");
        assert!(matches!(
            material_err,
            ServiceError::System(crate::error::system::SystemError::SystemNotReady)
        ));
    }
}
