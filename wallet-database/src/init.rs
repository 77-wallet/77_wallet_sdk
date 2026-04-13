use crate::DbPool;
use sqlx::{Pool, Sqlite, migrate::MigrateDatabase as _};
use std::sync::Arc;
use std::{future::Future, io};

#[derive(Debug, Clone, Copy)]
pub struct SqlitePoolConfig {
    pub reader_max_connections: u32,
    pub writer_max_connections: u32,
}

impl Default for SqlitePoolConfig {
    fn default() -> Self {
        Self { reader_max_connections: 4, writer_max_connections: 1 }
    }
}

#[derive(Debug, Clone)]
pub struct SqlitePoolProvider {
    pub uri: String,
    pub read_conn: DbPool,
    pub write_conn: DbPool,
}

#[derive(Debug, Clone)]
pub enum Migrator {
    Core,           // data.db
    ApiTransaction, // api_transaction.db
    ApiWallet,      // api_wallet.db
    Task,           // task.db
}
impl Migrator {
    pub fn migrator(&self) -> Result<sqlx::migrate::Migrator, crate::Error> {
        match self {
            Migrator::Core => Ok(sqlx::migrate!("./schema/core/migrations")),
            Migrator::ApiTransaction => Ok(sqlx::migrate!("./schema/api_transaction/migrations")),
            Migrator::ApiWallet => Ok(sqlx::migrate!("./schema/api_wallet/migrations")),
            Migrator::Task => Ok(sqlx::migrate!("./schema/task/migrations")),
        }
    }
}

impl SqlitePoolProvider {
    pub async fn new(uri: String, migrator: Migrator) -> Result<Self, crate::Error> {
        Self::new_with_config(uri, migrator, SqlitePoolConfig::default()).await
    }

    pub async fn new_with_config(
        uri: String,
        migrator: Migrator,
        config: SqlitePoolConfig,
    ) -> Result<Self, crate::Error> {
        let write_pool = Self::init_pool(&uri, config.writer_max_connections).await?;
        let read_pool = Self::init_pool(&uri, config.reader_max_connections).await?;

        // run migrations
        Self::run_migrate(write_pool.clone(), migrator).await?;

        Ok(Self { uri, read_conn: read_pool, write_conn: write_pool })
    }

    pub async fn run_migrate(pool: DbPool, migrator: Migrator) -> Result<(), crate::Error> {
        Self::run_migrate_internal(pool, migrator, |pool| async move {
            sqlx::query("ANALYZE").execute(pool.as_ref()).await
        })
        .await
    }

    async fn run_migrate_internal<F, Fut>(
        pool: DbPool,
        migrator: Migrator,
        analyze: F,
    ) -> Result<(), crate::Error>
    where
        F: FnOnce(DbPool) -> Fut,
        Fut: Future<Output = Result<(), sqlx::Error>>,
    {
        // run migraor
        if let Err(e) = migrator.migrator()?.run(pool.as_ref()).await {
            let msg = format!("migrate filed: remove files = {e}");
            tracing::error!(msg);
            panic!("{msg}");
        }

        // 执行ANALYZE，更新统计信息，优化查询计划
        if let Err(e) = analyze(pool.clone()).await {
            tracing::warn!("[run_migrate] ANALYZE failed but migration succeeded: {e}");
        }

        Ok(())
    }

    pub async fn init_pool(uri: &str, max_connections: u32) -> Result<DbPool, crate::Error> {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
        use std::{str::FromStr, time::Duration};

        if !sqlx::Sqlite::database_exists(uri).await.unwrap_or(false) {
            sqlx::Sqlite::create_database(uri)
                .await
                .map_err(|_| crate::DatabaseError::DatabaseCreateFailed)?;
        };

        tracing::debug!("[init_pool] data base uri: {uri}");

        // 配置 WAL + busy_timeout + NORMAL
        let opts = SqliteConnectOptions::from_str(uri)
            .map_err(|_| crate::DatabaseError::DatabaseConnectFailed)?
            .journal_mode(SqliteJournalMode::Wal) // 🚀 启用 WAL 模式
            .synchronous(SqliteSynchronous::Normal) // 🚀 更快的写，可靠性仍够用
            .busy_timeout(Duration::from_secs(5)) // 🚀 等锁最长 5 秒
            .create_if_missing(true);

        // 用连接池管理连接，避免单连接锁竞争
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(60)) // 🔥 增加获取连接等待时间
            .connect_with(opts)
            .await
            .map_err(|e| {
                tracing::error!("[init_database] connect error: {e}");
                crate::DatabaseError::DatabaseConnectFailed
            })?;

        Ok(Arc::new(pool))
    }

    pub fn get_pool(&self) -> Result<std::sync::Arc<Pool<Sqlite>>, crate::DatabaseError> {
        Ok(self.read_conn.clone())
    }

    pub fn get_read_pool(&self) -> Result<std::sync::Arc<Pool<Sqlite>>, crate::DatabaseError> {
        Ok(self.read_conn.clone())
    }

    pub fn get_write_pool(&self) -> Result<std::sync::Arc<Pool<Sqlite>>, crate::DatabaseError> {
        Ok(self.write_conn.clone())
    }

    pub fn get_uri(&self) -> String {
        self.uri.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_migrate_keeps_going_when_analyze_fails() {
        let pool = Arc::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .expect("open sqlite memory"),
        );

        let result = SqlitePoolProvider::run_migrate_internal(pool, Migrator::Core, |_pool| async {
            Err(sqlx::Error::Io(io::Error::other("analyze failed")))
        })
        .await;

        assert!(result.is_ok());
    }
}
