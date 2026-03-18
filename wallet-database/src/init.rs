use crate::DbPool;
use sqlx::{Pool, Sqlite, migrate::MigrateDatabase as _};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

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
    Core,      // data.db
    ApiFunds,  // api_funds.db
    ApiWallet, // api_wallet.db
    Task,      // task.db
}
impl Migrator {
    pub fn migrator(&self) -> Result<sqlx::migrate::Migrator, crate::Error> {
        match self {
            Migrator::Core => build_recursive_migrator("schema/core/migrations"),
            Migrator::ApiFunds => Ok(sqlx::migrate!("./schema/api_funds/migrations")),
            Migrator::ApiWallet => build_recursive_migrator("schema/api_wallet/migrations"),
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
        // run migraor
        if let Err(e) = migrator.migrator()?.run(pool.as_ref()).await {
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

fn build_recursive_migrator(rel_path: &str) -> Result<sqlx::migrate::Migrator, crate::Error> {
    let migrations = load_recursive_migrations(rel_path)?;
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(migrations),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    })
}

fn load_recursive_migrations(
    rel_path: &str,
) -> Result<Vec<sqlx::migrate::Migration>, crate::Error> {
    let mut migrations = BTreeMap::new();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let roots = [manifest_dir.join(rel_path)];

    for root in roots {
        if root.exists() {
            collect_migrations(&root, &mut migrations)?;
        }
    }

    Ok(migrations.into_values().collect())
}

fn collect_migrations(
    dir: &Path,
    migrations: &mut BTreeMap<i64, sqlx::migrate::Migration>,
) -> Result<(), crate::Error> {
    let entries = fs::read_dir(dir).map_err(|e| {
        crate::Error::Other(format!("error reading migration directory {}: {e}", dir.display()))
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            crate::Error::Other(format!(
                "error reading contents of migration directory {}: {e}",
                dir.display()
            ))
        })?;

        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| {
            crate::Error::Other(format!(
                "error getting metadata of migration path {}: {e}",
                path.display()
            ))
        })?;

        if metadata.is_dir() {
            collect_migrations(&path, migrations)?;
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let parts = file_name.splitn(2, '_').collect::<Vec<_>>();

        if parts.len() != 2 || !parts[1].ends_with(".sql") {
            continue;
        }

        let version: i64 = parts[0].parse().map_err(|_| {
            crate::Error::Other(format!(
                "error parsing migration filename {file_name:?}; expected integer version prefix"
            ))
        })?;

        let migration_type = sqlx::migrate::MigrationType::from_filename(parts[1]);
        let description =
            parts[1].trim_end_matches(migration_type.suffix()).replace('_', " ").to_owned();

        let sql = fs::read_to_string(&path).map_err(|e| {
            crate::Error::Other(format!(
                "error reading contents of migration {}: {e}",
                path.display()
            ))
        })?;
        let no_tx = sql.starts_with("-- no-transaction");

        let migration = sqlx::migrate::Migration::new(
            version,
            Cow::Owned(description),
            migration_type,
            Cow::Owned(sql),
            no_tx,
        );

        if migrations.insert(version, migration).is_some() {
            return Err(crate::Error::Other(format!(
                "duplicate migration version {version} found under {}",
                dir.display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn collect_migrations_recurses_into_nested_directories() {
        let suffix =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("time went backwards").as_nanos();
        let root = std::env::temp_dir().join(format!("wallet-db-migrations-{suffix}"));
        let nested = root.join("nested");

        fs::create_dir_all(&nested).expect("create temp dir");
        fs::write(root.join("20240101000001_root.sql"), "CREATE TABLE root_table(id INTEGER);")
            .expect("write root migration");
        fs::write(
            nested.join("20240101000002_nested.sql"),
            "CREATE TABLE nested_table(id INTEGER);",
        )
        .expect("write nested migration");
        fs::write(root.join("README.md"), "ignore me").expect("write ignored file");

        let mut migrations = BTreeMap::new();
        collect_migrations(&root, &mut migrations).expect("collect migrations");

        let versions: Vec<_> = migrations.keys().copied().collect();
        assert_eq!(versions, vec![20240101000001, 20240101000002]);

        fs::remove_dir_all(&root).ok();
    }
}
