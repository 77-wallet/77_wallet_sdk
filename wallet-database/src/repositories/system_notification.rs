use crate::{
    CoreDbPool,
    entities::system_notification::{CreateSystemNotificationEntity, SystemNotificationEntity},
    pagination::Pagination,
};

pub struct SystemNotificationRepo;

impl SystemNotificationRepo {
    pub async fn upsert(
        pool: &CoreDbPool,
        id: &str,
        r#type: &str,
        content: String,
        status: i8,
    ) -> Result<Vec<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::upsert(pool.as_ref(), id, r#type, content, status).await
    }

    pub async fn upsert_with_key_value(
        pool: &CoreDbPool,
        id: &str,
        r#type: &str,
        content: String,
        status: i8,
        key: Option<String>,
        value: Option<String>,
    ) -> Result<Vec<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::upsert_with_key_value(
            pool.as_ref(),
            id,
            r#type,
            content,
            status,
            key,
            value,
        )
        .await
    }

    pub async fn upsert_multi_with_key_value(
        pool: &CoreDbPool,
        reqs: &[CreateSystemNotificationEntity],
    ) -> Result<(), crate::Error> {
        SystemNotificationEntity::upsert_multi_with_key_value(pool.as_ref(), reqs).await
    }

    pub async fn list_page(
        pool: &CoreDbPool,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::system_notification_list_page(pool.as_ref(), page, page_size)
            .await
    }

    pub async fn update_status(
        pool: &CoreDbPool,
        id: Option<String>,
        status: i8,
    ) -> Result<(), crate::Error> {
        SystemNotificationEntity::update_system_notification_status(pool.as_ref(), id, status).await
    }

    pub async fn delete(pool: &CoreDbPool, id: &str) -> Result<(), crate::Error> {
        SystemNotificationEntity::delete_system_notification(pool.as_ref(), id).await
    }

    pub async fn count_unread(pool: &CoreDbPool) -> Result<i64, crate::Error> {
        SystemNotificationEntity::count_status_zero(pool.as_ref()).await
    }

    pub async fn find_by_key_value(
        pool: &CoreDbPool,
        key: Option<&str>,
        value: Option<&str>,
    ) -> Result<Option<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::detail(pool.as_ref(), key, value, None).await
    }

    pub async fn find_by_id(
        id: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::detail(pool.as_ref(), None, None, Some(id)).await
    }
}

#[cfg(test)]
mod tests {
    use crate::entities::system_notification::CreateSystemNotificationEntity;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn make_temp_dir(prefix: &str) -> String {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
            seq
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn system_notification_repo_list_and_detail_work_without_explicit_transaction() {
        let dir = make_temp_dir("wallet_db_repo_system_notification");
        let ctx = crate::SqliteContext::new(&dir, Some("data.db")).await.unwrap();
        let pool = ctx.get_pool().unwrap();
        let core_pool = crate::CoreDbPool::new(pool.clone());

        super::SystemNotificationRepo::upsert_multi_with_key_value(
            &core_pool,
            &[CreateSystemNotificationEntity::new(
                "n1",
                "system",
                "hello",
                0,
                Some("k".to_string()),
                Some("v".to_string()),
            )],
        )
        .await
        .unwrap();

        let detail =
            super::SystemNotificationRepo::find_by_id("n1", &core_pool).await.unwrap().unwrap();
        assert_eq!(detail.id, "n1");

        let page = super::SystemNotificationRepo::list_page(&core_pool, 0, 10).await.unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].id, "n1");

        let find =
            super::SystemNotificationRepo::find_by_key_value(&core_pool, Some("k"), Some("v"))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(find.id, "n1");
    }
}
