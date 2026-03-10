use crate::{
    ApiWalletDbPool,
    dao::expand_notify_state::ExpandNotifyStateDao,
    entities::expand_notify_state::{CreateExpandNotifyStateEntity, ExpandNotifyStateEntity},
};

pub struct ExpandNotifyStateRepo;

impl ExpandNotifyStateRepo {
    pub async fn get_by_uid_and_chain(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Option<ExpandNotifyStateEntity>, crate::Error> {
        ExpandNotifyStateDao::get_by_uid_and_chain(pool.as_ref(), uid, chain_code).await
    }

    pub async fn update_last_notified_page(
        pool: &ApiWalletDbPool,
        uid: &str,
        chain_code: &str,
        last_notified_page: i64,
    ) -> Result<(), crate::Error> {
        let req = CreateExpandNotifyStateEntity::new(uid, chain_code, last_notified_page);
        ExpandNotifyStateDao::upsert_last_notified_page(pool.as_ref(), req).await
    }
}
