use wallet_database::{
    entities::announcement::{AnnouncementEntity, CreateAnnouncementVo},
    pagination::Pagination,
    repositories::{announcement::AnnouncementRepoTrait, device::DeviceRepoTrait},
};
use wallet_transport_backend::request::AnnouncementListReq;

use crate::manager::Context;

pub struct AnnouncementService<T: AnnouncementRepoTrait> {
    repo: T,
}

impl<T: AnnouncementRepoTrait + DeviceRepoTrait> AnnouncementService<T> {
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn add(
        self,
        input: Vec<wallet_database::entities::announcement::CreateAnnouncementVo>,
    ) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;
        tx.add(input).await?;
        tx.commit_transaction().await?;
        Ok(())
    }

    pub async fn pull_announcement(mut self) -> Result<(), crate::error::ServiceError> {
        let tx = &mut self.repo;
        let backend = Context::get_global_backend_api()?;

        let list = tx.list().await?;

        if let Some(device) = tx.get_device_info().await?
            && let Some(uid) = device.uid
        {
            let req = AnnouncementListReq::new(uid, 0, 50);
            let res = backend.announcement_list(req).await?;

            let res_ids: std::collections::HashSet<_> =
                res.list.iter().map(|info| info.id.to_string()).collect();
            let to_delete: Vec<_> = list
                .into_iter()
                .filter(|item| !res_ids.contains(&item.id))
                .map(|item| item.id)
                .collect();

            for id in to_delete {
                tx.physical_delete(&id).await?;
            }

            let input = res
                .list
                .into_iter()
                .map(|info| CreateAnnouncementVo {
                    id: info.id.to_string(),
                    title: info.i18n.title,
                    content: info.i18n.content,
                    language: info.language,
                    status: 0,
                    send_time: info.send_time,
                })
                .collect();
            tx.add(input).await?;
        } else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        }

        Ok(())
    }

    pub async fn get_announcement_list(
        mut self,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AnnouncementEntity>, crate::error::ServiceError> {
        let res = self.repo.get_announcement_list(page, page_size).await?;

        Ok(res)
    }

    pub async fn read(self, id: Option<&str>) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;
        tx.read(id).await?;
        tx.commit_transaction().await?;
        Ok(())
    }

    pub async fn get_announcement_by_id(
        mut self,
        id: &str,
    ) -> Result<Option<AnnouncementEntity>, crate::error::ServiceError> {
        let res = self.repo.get_announcement_by_id(id).await?;
        Ok(res)
    }

    pub async fn physical_delete(self, id: &str) -> Result<(), crate::error::ServiceError> {
        let mut tx = self.repo.begin_transaction().await?;
        tx.physical_delete(id).await?;
        tx.commit_transaction().await?;
        Ok(())
    }
}
