use crate::{
    api::ReturnType, manager::WalletManager,
    response_vo::standard_wallet::address_book::AddressBookResp,
    service::address_book::AddressBookService,
};
use wallet_database::{entities::address_book::AddressBookEntity, pagination::Pagination};

impl WalletManager {
    pub async fn create_address_book(
        &self,
        name: String,
        address: String,
        chain_code: String,
    ) -> ReturnType<Option<AddressBookEntity>> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);
        service.create(&name, &address, &chain_code).await
    }

    pub async fn update_address_book(
        &self,
        id: u32,
        name: String,
        address: String,
        chain_code: String,
    ) -> ReturnType<Option<AddressBookEntity>> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);

        service.update(id, &name, &address, &chain_code).await
    }

    pub async fn delete_address_book(&self, id: i32) -> ReturnType<()> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);

        service.delete(id).await
    }

    pub async fn list_address_book(
        &self,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<AddressBookEntity>> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);

        service.lists(chain_code.as_deref(), page, page_size).await
    }

    pub async fn is_valid_address(&self, address: String, chain_code: String) -> ReturnType<()> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);

        service.check_address(address, chain_code).await
    }

    pub async fn find_by_address(
        &self,
        address: String,
        chain_code: String,
    ) -> ReturnType<AddressBookResp> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);
        service.find_by_address(address, chain_code).await
    }

    pub async fn address_status(&self, address: String, chain_code: String) -> ReturnType<i64> {
        let core_pool = self.ctx.core_pool()?;
        let service = AddressBookService::new(core_pool);

        service.address_status(address, chain_code).await
    }
}

mod test {

    #[tokio::test]
    async fn test_address_status() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = crate::testkit::env::get_manager().await?;
        let res = wallet_manager
            .address_status("TUDrRQ6zvwXhW3ScTxwGv8nwicLShVVWoF".to_string(), "tron".to_string())
            .await?;
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("res: {res:?}");
        Ok(())
    }
}
