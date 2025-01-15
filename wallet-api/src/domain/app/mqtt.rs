use wallet_database::repositories::{device::DeviceRepoTrait, ResourcesRepo};

use super::DeviceDomain;

pub(crate) struct MqttDomain {}

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
        crate::mqtt::init_mqtt_processor(
            &device.sn,
            &md5_sn,
            crate::mqtt::user_property::UserProperty::new(
                // &package_id.unwrap_or("77wallet".to_string()),
                &content, &client_id, &device.sn, &md5_sn,
            ),
            crate::mqtt::wrap_handle_eventloop,
        )
        .await?;
        Ok(())
    }
}
