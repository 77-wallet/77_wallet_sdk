//! Test-only entrypoints; fake transport and fixtures live in tests/harness.
use crate::{context::Context, error::service::ServiceError};

pub async fn initialize(ctx: &'static Context) -> Result<(), ServiceError> {
    crate::domain::node::NodeDomain::init_load_default_nodes_with_ctx(ctx).await?;
    crate::domain::api_wallet::chain::ApiChainDomain::init_api_chain_info(ctx).await
}

pub async fn pull_coins(ctx: &'static Context) -> Result<(), ServiceError> {
    crate::domain::api_wallet::coin::ApiCoinDomain::pull_api_coins(ctx).await.map(|_| ())
}

pub async fn register_addresses(
    ctx: &'static Context,
    request: wallet_transport_backend::request::api_wallet::address::ApiAddressInitReq,
) -> Result<(), ServiceError> {
    crate::infrastructure::expand_init::executor::do_init_with_ctx(ctx, request).await
}

pub fn core_pool(ctx: &'static Context) -> Result<wallet_database::CoreDbPool, ServiceError> {
    ctx.core_pool()
}

pub async fn sync_chains(ctx: &'static Context) -> Result<(), ServiceError> {
    crate::domain::api_wallet::chain::ApiChainDomain::sync_chains(ctx).await.map(|_| ())
}
