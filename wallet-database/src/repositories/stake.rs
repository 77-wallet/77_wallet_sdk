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
        Ok(StakeDao::add_unfreeze(stake, pool.write_ref()).await?)
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
        Ok(StakeDao::add_delegate(delegate, pool.write_ref()).await?)
    }

    pub async fn update_delegate(pool: &CoreDbPool, id: &str) -> Result<(), crate::Error> {
        Ok(StakeDao::update_delegate(id, pool.write_ref()).await?)
    }

    pub async fn delegate_list(
        pool: &CoreDbPool,
        owner_address: &str,
        resource_type: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateEntity>, crate::Error> {
        Ok(StakeDao::delegate_list(owner_address, resource_type, page, page_size, pool.read_pool())
            .await?)
    }

    pub async fn find_delegate_by_id(
        pool: &CoreDbPool,
        id: &str,
    ) -> Result<DelegateEntity, crate::Error> {
        Ok(StakeDao::find_delegate_by_id(id, pool.read_ref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::StakeRepo;
    use crate::{
        dao::stake::StakeDao, entities::stake::NewDelegateEntity,
        repositories::test_helper::setup_core_pool,
    };

    async fn ensure_stake_tables(pool: &crate::CoreDbPool) {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS delegate (
                id TEXT PRIMARY KEY,
                tx_hash TEXT NOT NULL,
                owner_address TEXT NOT NULL,
                receiver_address TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                amount TEXT NOT NULL,
                status INTEGER NOT NULL,
                lock INTEGER NOT NULL,
                lock_period INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT
            )
            "#,
        )
        .execute(pool.write_ref())
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS unfreeze (
                id TEXT PRIMARY KEY,
                tx_hash TEXT NOT NULL,
                owner_address TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                amount TEXT NOT NULL,
                freeze_time INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(pool.write_ref())
        .await
        .unwrap();
    }

    fn build_delegate(tx_hash: &str) -> NewDelegateEntity {
        NewDelegateEntity {
            tx_hash: tx_hash.to_string(),
            owner_address: "T_owner_stake".to_string(),
            receiver_address: "T_receiver_stake".to_string(),
            resource_type: "ENERGY".to_string(),
            amount: "1000".to_string(),
            lock: 0,
            lock_period: 0,
        }
    }

    #[tokio::test]
    async fn stake_repo_add_and_list_delegate_success() {
        let pool = setup_core_pool("wallet_db_stake_success").await;
        ensure_stake_tables(&pool).await;
        StakeRepo::add_delegate(&pool, build_delegate("tx_delegate_success")).await.unwrap();

        let page = StakeRepo::delegate_list(&pool, "T_owner_stake", "ENERGY", 0, 20).await.unwrap();
        assert_eq!(page.total_count, 1);
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].tx_hash, "tx_delegate_success");
    }

    #[tokio::test]
    async fn stake_repo_missing_delegate_id_returns_error() {
        let pool = setup_core_pool("wallet_db_stake_edge").await;
        ensure_stake_tables(&pool).await;
        let found = StakeRepo::find_delegate_by_id(&pool, "delegate_id_missing").await;
        assert!(found.is_err());
    }

    #[tokio::test]
    async fn stake_repo_tx_rollback_keeps_delegate_absent() {
        let pool = setup_core_pool("wallet_db_stake_rollback").await;
        ensure_stake_tables(&pool).await;

        let mut tx = pool.write_ref().begin().await.unwrap();
        StakeDao::add_delegate(build_delegate("tx_delegate_rb"), tx.as_mut()).await.unwrap();
        tx.rollback().await.unwrap();

        let page = StakeRepo::delegate_list(&pool, "T_owner_stake", "ENERGY", 0, 20).await.unwrap();
        assert_eq!(page.total_count, 0);
        assert!(page.data.is_empty());
    }
}
