use crate::{
    api::ReturnType,
    service::device::DeviceService,
    manager::WalletManager,
    request::devices::InitDeviceReq,
};

impl WalletManager {
    pub async fn init_device(&self, req: InitDeviceReq) -> ReturnType<Option<()>> {
        DeviceService::new(self.repo_factory.resource_repo()).init_device(req).await
    }
}
