use crate::messaging::notify::{FrontendNotifyEvent, event::NotifyEvent};
use wallet_database::repositories::{announcement::AnnouncementRepo, device::DeviceRepo};

pub struct AnnouncementDomain;

impl AnnouncementDomain {
    pub async fn pull_announcement(
        repo: &mut AnnouncementRepo<'_>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let list = repo.list().await?;

        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let Some(device) = DeviceRepo::get_device_info(pool, sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        let client_id = super::app::DeviceDomain::client_id_by_device(&device)?;
        let req = wallet_transport_backend::request::AnnouncementListReq::new(client_id, 0, 50);
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let res = backend.announcement_list(req).await?;

        let res_ids: std::collections::HashSet<_> =
            res.list.iter().map(|info| info.id.to_string()).collect();
        let to_delete: Vec<_> = list
            .into_iter()
            .filter(|item| !res_ids.contains(&item.id))
            .map(|item| item.id)
            .collect();

        for id in to_delete {
            repo.delete(&id).await?;
        }

        let input = res
            .list
            .into_iter()
            .map(|info| wallet_database::entities::announcement::CreateAnnouncementVo {
                id: info.id.to_string(),
                title: info.i18n.title,
                content: info.i18n.content,
                language: info.language,
                status: 0,
                send_time: info.send_time,
            })
            .collect();
        repo.update_existing(input).await?;

        let data = NotifyEvent::FetchBulletinMsg;
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }
}
