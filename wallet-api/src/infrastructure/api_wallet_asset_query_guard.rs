use once_cell::sync::Lazy;
use tokio::time::Duration;

use crate::{
    error::service::ServiceError,
    infrastructure::protected_query::ProtectedQuery,
    response_vo::standard_wallet::assets::{CoinAssets, GetAccountAssetsRes},
};

static ACCOUNT_ASSETS_QUERY: Lazy<ProtectedQuery<GetAccountAssetsRes>> =
    Lazy::new(ProtectedQuery::default);
static ASSET_DETAIL_QUERY: Lazy<ProtectedQuery<CoinAssets>> = Lazy::new(ProtectedQuery::default);

pub(crate) fn account_assets_key(
    wallet_address: &str,
    account_id: u32,
    chain_code: Option<&str>,
) -> String {
    format!(
        "api_wallet_account_assets:wallet={wallet_address};account_id={account_id};chain_code={}",
        chain_code.unwrap_or("none")
    )
}

pub(crate) fn asset_detail_key(
    address: &str,
    account_id: Option<u32>,
    chain_code: &str,
    token_address: Option<&str>,
) -> String {
    let account_part = account_id.map(|v| v.to_string()).unwrap_or_else(|| "none".to_string());
    format!(
        "api_wallet_asset_detail:address={address};account_id={account_part};chain_code={chain_code};token={}",
        token_address.unwrap_or("")
    )
}

pub(crate) async fn load_account_assets<F, Fut>(
    key: &str,
    fresh_ttl: Duration,
    query_fn: F,
) -> Result<GetAccountAssetsRes, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<GetAccountAssetsRes, ServiceError>>,
{
    ACCOUNT_ASSETS_QUERY.load(key, fresh_ttl, query_fn).await
}

pub(crate) async fn load_asset_detail<F, Fut>(
    key: &str,
    fresh_ttl: Duration,
    query_fn: F,
) -> Result<CoinAssets, ServiceError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<CoinAssets, ServiceError>>,
{
    ASSET_DETAIL_QUERY.load(key, fresh_ttl, query_fn).await
}

pub(crate) fn mark_account_assets_stale(wallet_address: &str, fresh_ttl: Duration) -> usize {
    ACCOUNT_ASSETS_QUERY
        .mark_matching_stale(
            &format!("api_wallet_account_assets:wallet={wallet_address};"),
            fresh_ttl,
        )
        .len()
}

pub(crate) fn mark_asset_detail_stale(
    address: &str,
    account_id: Option<u32>,
    chain_code: &str,
    token_address: Option<&str>,
    fresh_ttl: Duration,
) -> bool {
    let key = asset_detail_key(address, account_id, chain_code, token_address);
    ASSET_DETAIL_QUERY.mark_stale(&key, fresh_ttl)
}

#[cfg(test)]
mod tests {
    use super::{account_assets_key, asset_detail_key};

    #[test]
    fn account_assets_key_keeps_query_dimensions() {
        let key = account_assets_key("0xwallet", 7, Some("tron"));
        assert_eq!(key, "api_wallet_account_assets:wallet=0xwallet;account_id=7;chain_code=tron");
    }

    #[test]
    fn asset_detail_key_keeps_query_dimensions() {
        let key = asset_detail_key("TAddr", Some(1), "tron", Some("TToken"));
        assert_eq!(
            key,
            "api_wallet_asset_detail:address=TAddr;account_id=1;chain_code=tron;token=TToken"
        );
    }
}
