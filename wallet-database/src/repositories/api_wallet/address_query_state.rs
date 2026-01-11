use crate::{
    DbPool,
    dao::address_query_state::AddressQueryStateDao,
    entities::address_query_state::{
        AddressQueryStateEntity, AddressQueryStatus, CreateAddressQueryStateEntity,
    },
};

pub struct AddressQueryStateRepo {}

impl AddressQueryStateRepo {
    pub async fn upsert(
        pool: &DbPool,
        req: CreateAddressQueryStateEntity,
    ) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::upsert(pool.as_ref(), req).await?)
    }

    pub async fn get_by_uid_and_chain(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Option<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::get_by_uid_and_chain(pool.as_ref(), uid, chain_code).await?)
    }

    pub async fn update_status(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
        status: AddressQueryStatus,
    ) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::update_status(pool.as_ref(), uid, chain_code, status).await?)
    }

    pub async fn delete(pool: &DbPool, uid: &str, chain_code: &str) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::delete(pool.as_ref(), uid, chain_code).await?)
    }

    pub async fn delete_by_uid(pool: &DbPool, uid: &str) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::delete_by_uid(pool.as_ref(), uid).await?)
    }

    pub async fn list_by_uid(
        pool: &DbPool,
        uid: &str,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::list_by_uid(pool.as_ref(), uid).await?)
    }

    pub async fn list_by_status(
        pool: &DbPool,
        status: AddressQueryStatus,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::list_by_status(pool.as_ref(), status).await?)
    }
    
    /// 获取需要恢复的任务（Failed + 长时间未更新的Running）
    /// 长时间指：updated_at < now - 10 minutes
    pub async fn list_recoverable_tasks(
        pool: &DbPool,
        include_stuck_running: bool,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::list_recoverable_tasks(pool.as_ref(), include_stuck_running).await?)
    }

    pub async fn list_running_by_uid(
        pool: &DbPool,
        uid: &str,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::list_running_by_uid(pool.as_ref(), uid).await?)
    }

    pub async fn is_running(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<bool, crate::Error> {
        Ok(AddressQueryStateDao::is_running(pool.as_ref(), uid, chain_code).await?)
    }

    pub async fn count_by_status(
        pool: &DbPool,
        status: AddressQueryStatus,
    ) -> Result<i64, crate::Error> {
        Ok(AddressQueryStateDao::count_by_status(pool.as_ref(), status).await?)
    }

    pub async fn get_all(pool: &DbPool) -> Result<Vec<AddressQueryStateEntity>, crate::Error> {
        Ok(AddressQueryStateDao::get_all(pool.as_ref()).await?)
    }

    /// 更新最后处理的页码
    pub async fn update_last_page(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
        last_page: i64,
    ) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::update_last_page(
            pool.as_ref(),
            uid,
            chain_code,
            last_page,
        )
        .await?)
    }

    /// 更新总远程地址数
    pub async fn update_total_remote(
        pool: &DbPool,
        uid: &str,
        chain_code: &str,
        total_remote: i64,
    ) -> Result<(), crate::Error> {
        Ok(AddressQueryStateDao::update_total_remote(
            pool.as_ref(),
            uid,
            chain_code,
            total_remote,
        )
        .await?)
    }
}
