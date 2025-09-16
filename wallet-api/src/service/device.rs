use crate::domain::app::config::ConfigDomain;
use wallet_database::{
    entities::{
        config::config_key::APP_VERSION,
        device::{CreateDeviceEntity, DeviceEntity},
    },
    repositories::device::{DeviceRepo, DeviceRepoTrait},
};
use wallet_transport_backend::{consts::endpoint, request::DeviceInitReq};
// pub const APP_ID: &str = "bc7f694ee0a9488cada7d9308190fe45";
pub const APP_ID: &str = "ada7d9308190fe45";

use crate::{
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, task::Tasks},
    request::devices::InitDeviceReq,
};

pub struct DeviceService<T: DeviceRepoTrait> {
    pub repo: T,
    // keystore: wallet_crypto::Keystore
}

impl<T: DeviceRepoTrait> DeviceService<T> {
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn get_device_info(self) -> Result<Option<DeviceEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        Ok(DeviceRepo::get_device_info(pool).await?)
    }

    pub async fn init_device(self, req: InitDeviceReq) -> Result<Option<()>, crate::error::service::ServiceError> {
        let mut tx = self.repo;

        // let package_id = req.package_id.clone();
        let upsert_req = (&req).into();
        tx.upsert(upsert_req).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool).await? else {
            return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Device(crate::error::business::device::DeviceError::Uninitialized).into()));
        };

        if device.is_init == 0 {
            let task_req: DeviceInitReq = (&req).into();
            let task_data = BackendApiTaskData {
                endpoint: endpoint::DEVICE_INIT.to_string(),
                body: wallet_utils::serde_func::serde_to_value(&task_req)?,
            };

            Tasks::new().push(BackendApiTask::BackendApi(task_data)).send().await?;
        }

        let app_version =
            wallet_database::entities::config::AppVersion { app_version: req.app_version };
        ConfigDomain::set_config(APP_VERSION, &app_version.to_json_str()?).await?;

        Ok(None)
    }

    pub async fn add_device(self, req: CreateDeviceEntity) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        tx.upsert(req).await?;

        Ok(())
    }

    pub async fn unbind_device(self, sn: &str) -> Result<(), crate::error::service::ServiceError> {
        let task_data = BackendApiTaskData {
            endpoint: endpoint::KEYS_RESET.to_string(),
            body: serde_json::json!({
                "sn": sn,
            }),
        };

        Tasks::new().push(BackendApiTask::BackendApi(task_data)).send().await?;
        Ok(())
    }
}
