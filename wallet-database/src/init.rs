use crate::DbPool;
use sqlx::{Pool, Sqlite, migrate::MigrateDatabase as _};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SqlitePoolProvider {
    pub uri: String,
    pub conn: DbPool,
}

#[derive(Debug, Clone)]
pub enum Migrator {
    Core,     // data.db
    ApiFunds, // api_funds.db
}
impl Migrator {
    pub fn migrator(&self) -> sqlx::migrate::Migrator {
        match self {
            Migrator::Core => sqlx::migrate!("./schema/migrations"),
            Migrator::ApiFunds => sqlx::migrate!("./schema/api_funds/migrations"),
        }
    }
}

impl SqlitePoolProvider {
    pub async fn new(uri: String, migrator: Migrator) -> Result<Self, crate::Error> {
        let pool = Self::init_pool(&uri).await?;

        // run migrations
        Self::run_migrate(pool.clone(), migrator).await?;

        Ok(Self { uri, conn: pool })
    }

    pub async fn run_migrate(pool: DbPool, migrator: Migrator) -> Result<(), crate::Error> {
        // run migraor
        if let Err(e) = migrator.migrator().run(pool.as_ref()).await {
            let msg = format!("migrate filed: remove files = {e}");
            tracing::error!(msg);
            panic!("{msg}");
        }

        // 执行ANALYZE，更新统计信息，优化查询计划
        sqlx::query("ANALYZE").execute(pool.as_ref()).await.map_err(|e| {
            tracing::error!("[run_migrate] ANALYZE error: {e}");
            crate::DatabaseError::DatabaseConnectFailed
        })?;

        Ok(())
    }

    pub async fn init_pool(uri: &str) -> Result<DbPool, crate::Error> {
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
            .max_connections(20) // 可按需调整
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
        Ok(self.conn.clone())
    }

    pub fn get_uri(&self) -> String {
        self.uri.clone()
    }
}
