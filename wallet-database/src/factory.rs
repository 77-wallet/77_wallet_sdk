use sqlx::{Pool, Sqlite};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RepositoryFactory;

impl RepositoryFactory {
    pub fn repo(pool: Arc<Pool<Sqlite>>) -> crate::repositories::RepoCtx {
        crate::repositories::RepoCtx::new(pool)
    }
}
