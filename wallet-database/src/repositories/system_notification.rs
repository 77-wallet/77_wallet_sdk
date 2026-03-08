use crate::{
    CoreDbPool,
    entities::system_notification::{CreateSystemNotificationEntity, SystemNotificationEntity},
    pagination::Pagination,
    repositories::RepoCtx,
};

impl RepoCtx {
    pub async fn get_system_notification_detail(
        &mut self,
        id: &str,
    ) -> Result<Option<SystemNotificationEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::detail,
            None,
            None,
            Some(id)
        )
    }

    pub async fn upsert_system_notification(
        &mut self,
        id: &str,
        r#type: &str,
        content: String,
        status: i8,
    ) -> Result<Vec<SystemNotificationEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::upsert,
            id,
            r#type,
            content,
            status
        )
    }

    pub async fn upsert_system_notification_with_key_value(
        &mut self,
        id: &str,
        r#type: &str,
        content: String,
        status: i8,
        key: Option<String>,
        value: Option<String>,
    ) -> Result<Vec<SystemNotificationEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::upsert_with_key_value,
            id,
            r#type,
            content,
            status,
            key,
            value
        )
    }

    pub async fn upsert_multi_system_notification_with_key_value(
        &mut self,
        reqs: &[CreateSystemNotificationEntity],
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::upsert_multi_with_key_value,
            reqs
        )
    }

    pub async fn detail_system_notification_by_key(
        &mut self,
        key: Option<&str>,
        value: Option<&str>,
    ) -> Result<Option<SystemNotificationEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, SystemNotificationEntity::detail, key, value, None)
    }

    pub async fn list_system_notifications(
        &mut self,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<SystemNotificationEntity>, crate::Error> {
        let executor = self.pool_ref().as_ref();
        SystemNotificationEntity::system_notification_list_page(executor, page, page_size).await
    }

    pub async fn update_system_notification_status(
        &mut self,
        id: Option<String>,
        status: i8,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::update_system_notification_status,
            id,
            status
        )
    }

    pub async fn count_unread_system_notifications(&mut self) -> Result<i64, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, SystemNotificationEntity::count_status_zero,)
    }

    pub async fn delete_system_notification(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            SystemNotificationEntity::delete_system_notification,
            id
        )
    }
}

pub struct SystemNotificationRepo;

impl SystemNotificationRepo {
    pub async fn find_by_id(
        id: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<SystemNotificationEntity>, crate::Error> {
        SystemNotificationEntity::detail(pool.as_ref(), None, None, Some(id)).await
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        entities::system_notification::CreateSystemNotificationEntity, repositories::RepoCtx,
    };
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

        let mut repo = RepoCtx::new(pool.clone());
        repo.upsert_multi_system_notification_with_key_value(&[
            CreateSystemNotificationEntity::new(
                "n1",
                "system",
                "hello",
                0,
                Some("k".to_string()),
                Some("v".to_string()),
            ),
        ])
        .await
        .unwrap();

        let detail = repo.get_system_notification_detail("n1").await.unwrap().unwrap();
        assert_eq!(detail.id, "n1");

        let page = repo.list_system_notifications(0, 10).await.unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].id, "n1");

        let core_pool = crate::CoreDbPool::new(pool.clone());
        let find =
            super::SystemNotificationRepo::find_by_id("n1", &core_pool).await.unwrap().unwrap();
        assert_eq!(find.id, "n1");
    }
}
