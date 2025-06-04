use crate::{api::ReturnType, service::device::DeviceService};

impl crate::WalletManager {
    pub async fn init_device(&self, req: crate::InitDeviceReq) -> ReturnType<Option<()>> {
        DeviceService::new(self.repo_factory.resource_repo())
            .init_device(req)
            .await?
            .into()
    }

    pub async fn unbind_device(&self, sn: &str) -> ReturnType<()> {
        DeviceService::new(self.repo_factory.resource_repo())
            .unbind_device(sn)
            .await?
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::test::env::get_manager;
    use anyhow::Result;

    #[tokio::test]
    async fn test_unbind_device() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;

        let res = wallet_manager
            .unbind_device("5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d")
            .await;
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
