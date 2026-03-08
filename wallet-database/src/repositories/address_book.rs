use crate::{
    CoreDbPool, dao::address_book::AddressBookDao, entities::address_book::AddressBookEntity,
    pagination::Pagination,
};

pub struct AddressBookRepo;

impl AddressBookRepo {
    pub fn new(_db_pool: CoreDbPool) -> Self {
        Self
    }

    pub async fn insert(
        pool: &CoreDbPool,
        name: &str,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::insert(pool.as_ref(), name, address, chain_code).await?)
    }

    pub async fn update(
        pool: &CoreDbPool,
        id: u32,
        name: &str,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::update(pool.as_ref(), id, name, address, chain_code).await?)
    }

    pub async fn find_by_conditions(
        pool: &CoreDbPool,
        conditions: Vec<(&str, &str)>,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_condition(pool.as_ref(), conditions).await?)
    }

    pub async fn check_not_self(
        pool: &CoreDbPool,
        id: u32,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::check_not_self(pool.as_ref(), id, address, chain_code).await?)
    }

    pub async fn delete(pool: &CoreDbPool, id: i32) -> Result<(), crate::Error> {
        Ok(AddressBookDao::delete(pool.as_ref(), id).await?)
    }

    pub async fn list(
        pool: &CoreDbPool,
        chain_code: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::list(pool.clone().into_inner(), chain_code, page, page_size).await?)
    }

    pub async fn find_by_address(
        pool: &CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_by_address(pool.as_ref(), address, chain_code).await?)
    }

    pub async fn find_by_address_chain(
        pool: &CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AddressBookEntity>, crate::Error> {
        Ok(AddressBookDao::find_by_address(pool.as_ref(), address, chain_code).await?)
    }
}
