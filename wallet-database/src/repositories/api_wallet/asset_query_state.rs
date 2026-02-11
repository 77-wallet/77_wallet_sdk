use crate::{
    ApiWalletDbPool, dao::asset_query_state::AssetQueryStateDao,
    entities::asset_query_state::AssetQueryStateEntity,
};

pub struct AssetQueryStateRepo {}

impl AssetQueryStateRepo {
    pub async fn upsert_pending(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
        page: i64,
        index_list_json: &str,
    ) -> Result<(), crate::Error> {
        AssetQueryStateDao::upsert_pending(pool.as_ref(), uid, chain_code, page, index_list_json)
            .await
    }

    pub async fn claim_next(
        pool: &ApiWalletDbPool,
        include_stuck_running: bool,
    ) -> Result<Option<AssetQueryStateEntity>, crate::Error> {
        AssetQueryStateDao::claim_next(pool.as_ref(), include_stuck_running).await
    }

    pub async fn mark_done(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
        page: i64,
    ) -> Result<(), crate::Error> {
        AssetQueryStateDao::mark_done(pool.as_ref(), uid, chain_code, page).await
    }

    pub async fn mark_failed(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
        page: i64,
        err_msg: &str,
    ) -> Result<(), crate::Error> {
        AssetQueryStateDao::mark_failed(pool.as_ref(), uid, chain_code, page, err_msg).await
    }
}
