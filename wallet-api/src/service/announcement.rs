use wallet_database::{
    entities::announcement::AnnouncementEntity,
    pagination::Pagination,
    repositories::{RepoCtx, UnitOfWork, announcement::AnnouncementRepo},
};

use crate::domain::announcement::AnnouncementDomain;

pub struct AnnouncementService {
    repo: RepoCtx,
}

impl AnnouncementService {
    pub fn new(repo: impl Into<RepoCtx>) -> Self {
        Self { repo: repo.into() }
    }

    pub async fn add(
        self,
        input: Vec<wallet_database::entities::announcement::CreateAnnouncementVo>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        uow.begin().await?;
        {
            let mut repo = AnnouncementRepo::new(&mut uow);
            repo.add(input).await?;
        }
        uow.commit().await?;
        Ok(())
    }

    pub async fn pull_announcement(mut self) -> Result<(), crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        let mut repo = AnnouncementRepo::new(&mut uow);
        AnnouncementDomain::pull_announcement(&mut repo).await?;
        Ok(())
    }

    pub async fn get_announcement_list(
        self,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AnnouncementEntity>, crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        let repo = AnnouncementRepo::new(&mut uow);
        let res = repo.get_announcement_list(page, page_size).await?;

        Ok(res)
    }

    pub async fn read(self, id: Option<&str>) -> Result<(), crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        uow.begin().await?;
        {
            let mut repo = AnnouncementRepo::new(&mut uow);
            repo.read(id).await?;
        }
        uow.commit().await?;
        Ok(())
    }

    pub async fn get_announcement_by_id(
        self,
        id: &str,
    ) -> Result<Option<AnnouncementEntity>, crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        let repo = AnnouncementRepo::new(&mut uow);
        let res = repo.get_announcement_by_id(id).await?;
        Ok(res)
    }

    pub async fn physical_delete(
        self,
        id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut uow = UnitOfWork::from_ctx(self.repo);
        uow.begin().await?;
        {
            let mut repo = AnnouncementRepo::new(&mut uow);
            repo.delete(id).await?;
        }
        uow.commit().await?;
        Ok(())
    }
}
