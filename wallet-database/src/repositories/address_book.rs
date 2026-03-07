use crate::{
    CoreDbPool, dao::address_book::AddressBookDao, entities::address_book::AddressBookEntity,
    pagination::Pagination,
};

pub struct AddressBookRepo {
    pool: CoreDbPool,
}

impl AddressBookRepo {
    pub fn new(db_pool: CoreDbPool) -> Self {
        Self { pool: db_pool }
    }
}

impl AddressBookRepo {
    pub async fn insert(
        &mut self,
        name: &str,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::insert(self.pool.as_ref(), name, address, chain_code).await?)
    }

    pub async fn update(
        &mut self,
        id: u32,
        name: &str,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::update(self.pool.as_ref(), id, name, address, chain_code).await?)
    }

    pub async fn find_by_conditions(
        &mut self,
        conditions: Vec<(&str, &str)>,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_condition(self.pool.as_ref(), conditions).await?)
    }

    pub async fn check_not_self(
        &mut self,
        id: u32,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::check_not_self(self.pool.as_ref(), id, address, chain_code).await?)
    }

    pub async fn delete(&mut self, id: i32) -> Result<(), crate::Error> {
        Ok(AddressBookDao::delete(self.pool.as_ref(), id).await?)
    }

    pub async fn list(
        &mut self,
        chain_code: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::list(self.pool.clone().into_inner(), chain_code, page, page_size).await?)
    }

    pub async fn find_by_address(
        &mut self,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_by_address(self.pool.as_ref(), address, chain_code).await?)
    }

    pub async fn find_by_address_chain(
        pool: &CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_by_address(pool.as_ref(), address, chain_code).await?)
    }
}
