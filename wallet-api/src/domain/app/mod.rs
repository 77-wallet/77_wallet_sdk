pub mod config;
use crate::service::device::APP_ID;

pub struct AppDomain<T> {
    phantom: std::marker::PhantomData<T>,
}
impl<T> Default for AppDomain<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> AppDomain<T> {
    pub fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }
}

pub struct DeviceDomain;

impl DeviceDomain {
    pub fn device_content(
        device: &wallet_database::entities::device::DeviceEntity,
    ) -> Result<String, crate::ServiceError> {
        let identifier = DeviceDomain::device_identifier(&device.sn, &device.device_type);

        Ok(wallet_utils::ecb::Aes128EcbCryptor::new(APP_ID)
            .unwrap()
            .encrypt(&identifier)
            .unwrap())
    }

    //  设备的唯一标识:(app_id,sn,device_type)
    pub fn device_identifier(sn: &str, device_type: &str) -> String {
        format!("{}_{}_{}", APP_ID, sn, device_type)
    }

    // 根据设备唯一标识计算：client_id
    pub fn client_id_by_identifier(identifier: &str) -> String {
        wallet_utils::md5(identifier)
    }

    pub fn client_id_by_device(
        device: &wallet_database::entities::device::DeviceEntity,
    ) -> Result<String, crate::ServiceError> {
        let identifier = DeviceDomain::device_identifier(&device.sn, &device.device_type);

        Ok(DeviceDomain::client_id_by_identifier(&identifier))
    }
}
