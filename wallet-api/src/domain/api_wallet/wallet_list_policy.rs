use once_cell::sync::Lazy;
use wallet_database::entities::api_wallet::ApiWalletEntity;

use crate::{
    config::runtime_defaults,
    error::service::ServiceError,
    infrastructure::protected_query::ProtectedQuery,
    response_vo::api_wallet::wallet::{ApiWalletItem, ApiWalletList, WalletInfo},
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

pub(crate) fn invalidate_api_wallet_list() {
    API_WALLET_LIST_QUERY.invalidate_key(API_WALLET_LIST_KEY);
}

/// 从缓存中移除指定地址的钱包，避免 invalidate 后全量重查 DB
pub(crate) fn remove_wallet_from_cache(address: &str) {
    if let Some(mut list) = API_WALLET_LIST_QUERY.get_stale(API_WALLET_LIST_KEY) {
        let before = list.len();
        list.retain(|item| {
            let recharge_match =
                item.recharge_wallet.as_ref().map_or(false, |w| w.address == address);
            let withdraw_match =
                item.withdraw_wallet.as_ref().map_or(false, |w| w.address == address);
            !recharge_match && !withdraw_match
        });
        // 级联：移除 withdraw_wallet 被保留但 recharge_wallet 被删除的孤儿项
        // 以及其他因级联删除产生的半空项（根据业务场景决定是否保留）
        if list.len() != before {
            API_WALLET_LIST_QUERY.set(API_WALLET_LIST_KEY, &list);
        }
    }
}

/// 将新创建/导入的钱包追加到缓存中，避免 invalidate 后全量重查 DB
pub(crate) fn add_wallets_to_cache(wallets: &[ApiWalletEntity]) {
    if wallets.is_empty() {
        return;
    }
    let mut list: ApiWalletList = API_WALLET_LIST_QUERY
        .get_stale(API_WALLET_LIST_KEY)
        .unwrap_or_else(|| ApiWalletList(Vec::new()));
    let mut balance_list = std::collections::HashMap::new();
    // 从现有缓存中收集余额信息，避免丢失
    for item in list.iter() {
        if let Some(r) = &item.recharge_wallet {
            balance_list.insert(r.address.clone(), r.balance.clone());
        }
        if let Some(w) = &item.withdraw_wallet {
            balance_list.insert(w.address.clone(), w.balance.clone());
        }
    }

    use wallet_database::entities::api_wallet::ApiWalletType;
    for e in wallets {
        let mut wallet: WalletInfo = e.into();
        wallet.balance = balance_list.get(&e.address).cloned().unwrap_or_default();

        match e.api_wallet_type {
            ApiWalletType::SubAccount => {
                if let Some(binding_address) = &e.binding_address
                    && let Some(item) = list.iter_mut().find(|item| {
                        item.withdraw_wallet
                            .as_ref()
                            .map(|w| &w.address == binding_address)
                            .unwrap_or(false)
                    })
                {
                    item.recharge_wallet = Some(wallet);
                } else {
                    list.push(ApiWalletItem {
                        recharge_wallet: Some(wallet),
                        withdraw_wallet: None,
                    });
                }
            }
            ApiWalletType::Withdrawal => {
                if let Some(binding_address) = &e.binding_address
                    && let Some(item) = list.iter_mut().find(|item| {
                        item.recharge_wallet
                            .as_ref()
                            .map(|r| &r.address == binding_address)
                            .unwrap_or(false)
                    })
                {
                    item.withdraw_wallet = Some(wallet);
                } else {
                    list.push(ApiWalletItem {
                        recharge_wallet: None,
                        withdraw_wallet: Some(wallet),
                    });
                }
            }
        }
    }
    API_WALLET_LIST_QUERY.set(API_WALLET_LIST_KEY, &list);
}

/// 精准更新缓存中指定钱包的名字，避免 invalidate 后全量重查 DB
pub(crate) fn rename_wallet_in_cache(address: &str, new_name: &str) {
    if let Some(mut list) = API_WALLET_LIST_QUERY.get_stale(API_WALLET_LIST_KEY) {
        let mut changed = false;
        for item in list.iter_mut() {
            if let Some(r) = item.recharge_wallet.as_mut()
                && r.address == address
            {
                r.name = new_name.to_string();
                changed = true;
            }
            if let Some(w) = item.withdraw_wallet.as_mut()
                && w.address == address
            {
                w.name = new_name.to_string();
                changed = true;
            }
        }
        if changed {
            API_WALLET_LIST_QUERY.set(API_WALLET_LIST_KEY, &list);
        }
    }
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

    #[tokio::test]
    async fn api_wallet_list_policy_drops_deleted_wallet_after_invalidation() {
        let cached = super::load_api_wallet_list(|| async {
            Ok(ApiWalletList(vec![crate::response_vo::api_wallet::wallet::ApiWalletItem {
                recharge_wallet: None,
                withdraw_wallet: None,
            }]))
        })
        .await
        .expect("initial list query ok");
        assert_eq!(cached.len(), 1);

        super::invalidate_api_wallet_list();

        let reloaded = super::load_api_wallet_list(|| async { Ok(ApiWalletList::new()) })
            .await
            .expect("reloaded list query ok");

        assert!(reloaded.is_empty());
    }
}
