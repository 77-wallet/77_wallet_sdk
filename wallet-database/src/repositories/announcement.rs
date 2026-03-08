use crate::{
    CoreDbPool,
    entities::announcement::{AnnouncementEntity, CreateAnnouncementVo},
    pagination::Pagination,
};

pub struct AnnouncementRepo {
    pool: CoreDbPool,
}

impl AnnouncementRepo {
    pub fn new(pool: CoreDbPool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, input: Vec<CreateAnnouncementVo>) -> Result<(), crate::Error> {
        AnnouncementEntity::upsert(self.pool.as_ref(), input).await
    }

    pub async fn update_existing(
        &self,
        input: Vec<CreateAnnouncementVo>,
    ) -> Result<(), crate::Error> {
        AnnouncementEntity::update_existing(self.pool.as_ref(), input).await
    }

    pub async fn list(&self) -> Result<Vec<AnnouncementEntity>, crate::Error> {
        AnnouncementEntity::list(self.pool.as_ref()).await
    }

    pub async fn get_announcement_list(
        &self,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AnnouncementEntity>, crate::Error> {
        AnnouncementEntity::get_announcement_list(self.pool.as_ref(), page, page_size).await
    }

    pub async fn get_announcement_by_id(
        &self,
        id: &str,
    ) -> Result<Option<AnnouncementEntity>, crate::Error> {
        AnnouncementEntity::get_announcement_by_id(self.pool.as_ref(), id).await
    }

    pub async fn read(&self, id: Option<&str>) -> Result<(), crate::Error> {
        AnnouncementEntity::update_status(self.pool.as_ref(), id, 1).await?;
        Ok(())
    }

    pub async fn count_unread(&self) -> Result<i64, crate::Error> {
        AnnouncementEntity::count_status_zero(self.pool.as_ref()).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), crate::Error> {
        AnnouncementEntity::physical_delete(self.pool.as_ref(), id).await
    }
}

impl AnnouncementRepo {
    pub async fn count_unread_by_pool(pool: &CoreDbPool) -> Result<i64, crate::Error> {
        AnnouncementEntity::count_status_zero(pool.as_ref()).await
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        entities::announcement::CreateAnnouncementVo, repositories::announcement::AnnouncementRepo,
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
    async fn announcement_repo_queries_work_without_explicit_transaction() {
        let dir = make_temp_dir("wallet_db_repo_announcement");
        let ctx = crate::SqliteContext::new(&dir, Some("data.db")).await.unwrap();
        let pool = ctx.get_pool().unwrap();

        let repo = AnnouncementRepo::new(crate::CoreDbPool::new(pool));
        repo.add(vec![CreateAnnouncementVo {
            id: "a1".to_string(),
            title: "title".to_string(),
            content: "content".to_string(),
            language: "en".to_string(),
            status: 0,
            send_time: None,
        }])
        .await
        .unwrap();

        let one = repo.get_announcement_by_id("a1").await.unwrap().unwrap();
        assert_eq!(one.id, "a1");

        let page = repo.get_announcement_list(0, 10).await.unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].id, "a1");
    }
}
