use crate::{
    CoreDbPool,
    dao::stake::StakeDao,
    entities::stake::{DelegateEntity, NewDelegateEntity, NewUnFreezeEntity},
    pagination::Pagination,
};

pub struct StakeRepo;

impl StakeRepo {
    pub async fn add_unfreeze(
        pool: &CoreDbPool,
        stake: NewUnFreezeEntity,
    ) -> Result<(), crate::Error> {
        Ok(StakeDao::add_unfreeze(stake, pool.as_ref()).await?)
    }

    // pub async fn unfreeze_list(
    //     &self,
    //     owner: &str,
    //     resource_type: &str,
    // ) -> Result<Pagination<UnFreezeEntity>, crate::Error> {
    //     let pool = self.repo.pool();
    //     Ok(stake::unfreeze_list(owner, resource_type, page, page_size, &pool).await?)
    // }

    pub async fn add_delegate(
        pool: &CoreDbPool,
        delegate: NewDelegateEntity,
    ) -> Result<(), crate::Error> {
        Ok(StakeDao::add_delegate(delegate, pool.as_ref()).await?)
    }

    pub async fn update_delegate(pool: &CoreDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(StakeDao::update_delegate(id, pool.as_ref()).await?)
    }

    pub async fn delegate_list(
        pool: &CoreDbPool,
        owner_address: &str,
        resource_type: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateEntity>, crate::Error> {
        Ok(StakeDao::delegate_list(
            owner_address,
            resource_type,
            page,
            page_size,
            pool.clone().into_inner(),
        )
        .await?)
    }

    pub async fn find_delegate_by_id(
        pool: &CoreDbPool,
        id: &str,
    ) -> Result<DelegateEntity, crate::Error> {
        Ok(StakeDao::find_delegate_by_id(id, pool.as_ref()).await?)
    }
}
