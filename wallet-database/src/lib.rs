mod error;
pub use error::{DatabaseError, Error};
pub mod dao;
pub mod db;
pub mod db_pool;
pub use db_pool::{
    ApiFundsDbPool, ApiTransactionDbPool, ApiWalletDbPool, CoreDbPool, DbPool, TaskDbPool,
};
pub mod entities;
mod init;
pub use init::SqlitePoolConfig;
pub mod pagination;
pub mod repositories;
pub(crate) mod sql_utils;

// database pool
pub use wallet_tree::KdfAlgorithm;

#[derive(Debug, Clone)]
pub struct SqliteContext {
    pub sqlite_provider: crate::init::SqlitePoolProvider,
}

impl SqliteContext {
    pub async fn new(db_path: &str, db_name: Option<&str>) -> Result<Self, crate::Error> {
        Self::new_with_config(db_path, db_name, SqlitePoolConfig::default()).await
    }

    pub async fn new_with_config(
        db_path: &str,
        db_name: Option<&str>,
        config: SqlitePoolConfig,
    ) -> Result<Self, crate::Error> {
        let db_name = db_name.unwrap_or("data.db");
        let uri = format!("{db_path}/{db_name}");

        // 根据db_name选择对应的Migrator
        let migrator = match db_name {
            "data.db" => crate::init::Migrator::Core,
            "api_funds.db" => crate::init::Migrator::ApiFunds,
            "api_transaction.db" => crate::init::Migrator::ApiTransaction,
            "api_wallet.db" => crate::init::Migrator::ApiWallet,
            "task.db" => crate::init::Migrator::Task,
            _ => {
                return Err(crate::Error::Database(
                    crate::error::database::DatabaseError::InvalidDatabaseName(db_name.to_string()),
                ));
            }
        };

        let provider =
            crate::init::SqlitePoolProvider::new_with_config(uri, migrator, config).await?;

        Ok(SqliteContext { sqlite_provider: provider })
    }

    pub fn get_pool(&self) -> Result<std::sync::Arc<sqlx::SqlitePool>, crate::Error> {
        Ok(self.sqlite_provider.get_pool()?)
    }

    pub fn into_core_db_pool(self) -> Result<CoreDbPool, crate::Error> {
        Ok(CoreDbPool::new_split(
            self.sqlite_provider.get_read_pool()?,
            self.sqlite_provider.get_write_pool()?,
        ))
    }

    pub fn into_task_db_pool(self) -> Result<TaskDbPool, crate::Error> {
        Ok(TaskDbPool::new_split(
            self.sqlite_provider.get_read_pool()?,
            self.sqlite_provider.get_write_pool()?,
        ))
    }

    pub fn into_transaction_db_pool(self) -> Result<ApiTransactionDbPool, crate::Error> {
        Ok(ApiTransactionDbPool::new_split(
            self.sqlite_provider.get_read_pool()?,
            self.sqlite_provider.get_write_pool()?,
        ))
    }

    pub fn into_collect_db_pool(self) -> Result<ApiFundsDbPool, crate::Error> {
        self.into_transaction_db_pool()
    }

    pub fn into_api_wallet_db_pool(self) -> Result<ApiWalletDbPool, crate::Error> {
        Ok(ApiWalletDbPool::new_split(
            self.sqlite_provider.get_read_pool()?,
            self.sqlite_provider.get_write_pool()?,
        ))
    }
}

pub(crate) fn any_in_collection<T, I>(collection: I, placeholder: &str) -> String
where
    T: std::fmt::Display,
    I: IntoIterator<Item = T>,
{
    let mut iter = collection.into_iter().peekable();
    let mut any = String::new();

    while let Some(item) = iter.next() {
        any.push_str(&format!("{}", item));
        if iter.peek().is_some() {
            any.push_str(placeholder);
        }
    }

    any
}
