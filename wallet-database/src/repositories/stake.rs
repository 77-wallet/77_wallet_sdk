use crate::{
    CoreDbPool,
    dao::stake,
    entities::stake::{DelegateEntity, NewDelegateEntity, NewUnFreezeEntity},
    pagination::Pagination,
};

pub struct StakeRepo {
    pool: CoreDbPool,
}

impl StakeRepo {
    pub fn new(db_pool: crate::CoreDbPool) -> Self {
        Self { pool: db_pool }
    }
}

impl StakeRepo {
    pub async fn add_unfreeze(&self, stake: NewUnFreezeEntity) -> Result<(), crate::Error> {
        Ok(stake::add_unfreeze(stake, self.pool.as_ref()).await?)
    }

    // pub async fn unfreeze_list(
    //     &self,
    //     owner: &str,
    //     resource_type: &str,
    // ) -> Result<Pagination<UnFreezeEntity>, crate::Error> {
    //     let pool = self.repo.pool();
    //     Ok(stake::unfreeze_list(owner, resource_type, page, page_size, &pool).await?)
    // }

    pub async fn add_delegate(&self, delegate: NewDelegateEntity) -> Result<(), crate::Error> {
        Ok(stake::add_delegate(delegate, self.pool.as_ref()).await?)
    }

    pub async fn update_delegate(&self, id: &str) -> Result<(), crate::Error> {
        Ok(stake::update_delegate(id, self.pool.as_ref()).await?)
    }

    pub async fn delegate_list(
        &self,
        owner_address: &str,
        resource_type: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateEntity>, crate::Error> {
        Ok(stake::delegate_list(
            owner_address,
            resource_type,
            page,
            page_size,
            self.pool.clone().into_inner(),
        )
        .await?)
    }

    pub async fn find_delegate_by_id(&self, id: &str) -> Result<DelegateEntity, crate::Error> {
        Ok(stake::find_delegate_by_id(id, self.pool.as_ref()).await?)
    }
}
