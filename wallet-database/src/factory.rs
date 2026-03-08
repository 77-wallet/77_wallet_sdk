use sqlx::{Pool, Sqlite};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RepositoryFactory {
    db_pool: crate::DbPool,
}

impl RepositoryFactory {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self { db_pool }
    }

    pub fn repo(pool: Arc<Pool<Sqlite>>) -> crate::repositories::RepoCtx {
        crate::repositories::RepoCtx::new(pool)
    }

    pub fn resource_repo(&self) -> crate::repositories::RepoCtx {
        crate::repositories::RepoCtx::new(self.db_pool.clone())
    }

    pub fn multisig_account_repo(
        &self,
    ) -> crate::repositories::multisig_account::MultisigAccountRepo {
        crate::repositories::multisig_account::MultisigAccountRepo::new(crate::CoreDbPool::new(
            self.db_pool.clone(),
        ))
    }
}
