use crate::infrastructure::mqtt::{init::init_mqtt_processor, property::UserProperty};
use wallet_database::{
    dao::config::ConfigDao,
    entities::config::{config_key::APP_VERSION, AppVersion},
    repositories::{device::DeviceRepoTrait, ResourcesRepo},
};

use super::DeviceDomain;

pub(crate) struct MqttDomain;

impl MqttDomain {
    pub(crate) async fn init(repo: &mut ResourcesRepo) -> Result<(), crate::ServiceError> {
        let device =
            DeviceRepoTrait::get_device_info(repo)
                .await?
                .ok_or(crate::BusinessError::Device(
                    crate::DeviceError::Uninitialized,
                ))?;
        let content = DeviceDomain::device_content(&device)?;
        let client_id = DeviceDomain::client_id_by_device(&device)?;
        let md5_sn = DeviceDomain::md5_sn(&device.sn);

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let app_version = ConfigDao::find_by_key(APP_VERSION, pool.as_ref())
            .await?
            .ok_or(crate::ServiceError::Business(crate::BusinessError::Config(
                crate::ConfigError::NotFound(APP_VERSION.to_owned()),
            )))?;

        let app_version = AppVersion::try_from(app_version.value)?;

        init_mqtt_processor(UserProperty::new(
            &content,
            &client_id,
            &device.sn,
            &md5_sn,
            &app_version.app_version,
        ))
        .await?;

        Ok(())
    }

    pub(crate) async fn process_unconfirm_msg(client_id: &str) -> Result<(), crate::ServiceError> {
        let backend_api = crate::manager::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;

        let data = backend_api
            .send_msg_query_unconfirm_msg(
                cryptor,
                &wallet_transport_backend::request::SendMsgQueryUnconfirmMsgReq {
                    client_id: client_id.to_string(),
                },
            )
            .await?
            .list;
        crate::service::jpush::JPushService::jpush_multi(data, "API").await?;

        Ok(())
    }
}
