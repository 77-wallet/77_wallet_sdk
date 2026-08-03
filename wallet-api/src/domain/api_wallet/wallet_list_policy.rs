use once_cell::sync::Lazy;

use crate::{
    config::runtime_defaults, error::service::ServiceError,
    infrastructure::protected_query::ProtectedQuery,
    response_vo::api_wallet::wallet::ApiWalletList,
};

static API_WALLET_LIST_QUERY: Lazy<ProtectedQuery<ApiWalletList>> =
    Lazy::new(ProtectedQuery::default);

const API_WALLET_LIST_KEY: &str = "api_wallet_list:full";

pub(crate) async fn load_api_wallet_list<F, Fut>(query_fn: F) -> Result<ApiWalletList, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<ApiWalletList, ServiceError>>,
{
    let fresh_ttl = runtime_defaults::api_assets().total_cache_ttl;
    API_WALLET_LIST_QUERY
        .load(API_WALLET_LIST_KEY, fresh_ttl, || async move {
            match query_fn().await {
                Ok(list) => Ok(list),
                Err(err) => {
                    if err.is_transient_db_pool_timeout() {
                        tracing::warn!(
                            metric = "api_wallet_list_db_pool_timeout",
                            err = %err,
                            "api wallet list query hit db pool timeout"
                        );
                    }
                    Err(err)
                }
            }
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use tokio::time::{Duration, sleep};

    use crate::{
        error::service::ServiceError, infrastructure::protected_query::ProtectedQuery,
        response_vo::api_wallet::wallet::ApiWalletList,
    };

    fn unique_key(prefix: &str) -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        format!("{prefix}-{}", ts.as_nanos())
    }

    #[tokio::test]
    async fn api_wallet_list_policy_deduplicates_entrypoint_loads() {
        let hits = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..32 {
            let hits = hits.clone();
            tasks.push(tokio::spawn(async move {
                super::load_api_wallet_list(|| async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_millis(20)).await;
                    Ok(ApiWalletList::new())
                })
                .await
            }));
        }

        for task in tasks {
            let res = task.await.expect("join ok").expect("list query ok");
            assert!(res.is_empty());
        }

        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn api_wallet_list_query_deduplicates_concurrent_loads() {
        let protected = Arc::new(ProtectedQuery::<ApiWalletList>::default());
        let key = unique_key("api-wallet-list");
        let hits = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..32 {
            let protected = protected.clone();
            let key = key.clone();
            let hits = hits.clone();
            tasks.push(tokio::spawn(async move {
                protected
                    .load(&key, Duration::from_secs(5), || async move {
                        hits.fetch_add(1, Ordering::SeqCst);
                        sleep(Duration::from_millis(20)).await;
                        Ok(ApiWalletList::new())
                    })
                    .await
            }));
        }

        for task in tasks {
            let res = task.await.expect("join ok").expect("list query ok");
            assert!(res.is_empty());
        }

        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn api_wallet_list_query_returns_stale_on_timeout() {
        let protected = ProtectedQuery::<ApiWalletList>::default();
        let key = unique_key("api-wallet-list-stale");

        let first = protected
            .load(&key, Duration::from_secs(60), || async { Ok(ApiWalletList::new()) })
            .await
            .expect("initial query ok");
        assert!(first.is_empty());

        protected.invalidate_flight(&key);
        protected.mark_key_stale(&key, Duration::from_secs(3600));

        let stale = protected
            .load(&key, Duration::from_millis(0), || async { Err(ServiceError::Timeout) })
            .await
            .expect("timeout should return stale list");
        assert!(stale.is_empty());
    }
}
