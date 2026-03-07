use crate::{
    entities::announcement::{AnnouncementEntity, CreateAnnouncementVo},
    pagination::Pagination,
};

#[async_trait::async_trait]
pub trait AnnouncementRepoTrait: super::TransactionTrait {
    async fn add(&mut self, input: Vec<CreateAnnouncementVo>) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::upsert, input)
    }

    async fn update_existing(
        &mut self,
        input: Vec<CreateAnnouncementVo>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::update_existing, input)
    }

    async fn list(&mut self) -> Result<Vec<AnnouncementEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::list,)
    }

    async fn get_announcement_list(
        &mut self,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AnnouncementEntity>, crate::Error> {
        let executor = self.get_db_pool();
        AnnouncementEntity::get_announcement_list(executor, page, page_size).await
    }

    async fn get_announcement_by_id(
        &mut self,
        id: &str,
    ) -> Result<Option<AnnouncementEntity>, crate::Error> {
        let executor = self.get_db_pool();
        AnnouncementEntity::get_announcement_by_id(executor, id).await
    }

    async fn read(&mut self, id: Option<&str>) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::update_status, id, 1)?;
        Ok(())
    }

    async fn count_unread_status(&mut self) -> Result<i64, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::count_status_zero,)
    }

    async fn physical_delete(&mut self, id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, AnnouncementEntity::physical_delete, id)
    }
}

#[cfg(test)]
mod tests {
    use super::AnnouncementRepoTrait;
    use crate::{repositories::ResourcesRepo, entities::announcement::CreateAnnouncementVo};
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

        let mut repo = ResourcesRepo::new(pool);
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
