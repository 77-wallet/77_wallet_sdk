use sqlx::Sqlite;
use std::time::Instant;

pub type DbPool = std::sync::Arc<sqlx::Pool<Sqlite>>;

#[derive(Clone, Debug)]
struct SplitDbPool {
    read_pool: DbPool,
    write_pool: DbPool,
    write_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
}

#[derive(Clone, Debug)]
pub struct CoreDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct TaskDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct ApiTransactionDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct ApiWalletDbPool(std::sync::Arc<SplitDbPool>);

macro_rules! impl_split_pool_wrapper {
    ($name:ident) => {
        impl $name {
            // Backward compatible constructor: read/write share the same pool.
            pub fn new(pool: DbPool) -> Self {
                Self::new_split(pool.clone(), pool)
            }

            pub fn new_split(read_pool: DbPool, write_pool: DbPool) -> Self {
                Self(std::sync::Arc::new(SplitDbPool {
                    read_pool,
                    write_pool,
                    write_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                }))
            }

            pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.read_pool.as_ref()
            }

            pub fn read_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.read_pool.as_ref()
            }

            pub fn write_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.write_pool.as_ref()
            }

            pub fn read_pool(&self) -> DbPool {
                self.0.read_pool.clone()
            }

            pub fn write_pool(&self) -> DbPool {
                self.0.write_pool.clone()
            }

            pub async fn lock_write(&self) -> tokio::sync::OwnedMutexGuard<()> {
                self.lock_write_with_metric("unspecified").await
            }

            pub async fn lock_write_with_metric(
                &self,
                op: &str,
            ) -> tokio::sync::OwnedMutexGuard<()> {
                let wait_start = Instant::now();
                let guard = self.0.write_gate.clone().lock_owned().await;
                let wait_ms = wait_start.elapsed().as_secs_f64() * 1000.0;
                tracing::info!(
                    metric = "writer_gate_wait_ms",
                    db_pool = stringify!($name),
                    op = %op,
                    value_ms = %wait_ms,
                    "writer gate acquired"
                );
                guard
            }

            // Compatibility helper used by legacy transaction code paths.
            pub fn into_inner(&self) -> DbPool {
                self.0.write_pool.clone()
            }
        }
    };
}

impl_split_pool_wrapper!(CoreDbPool);
impl_split_pool_wrapper!(TaskDbPool);
impl_split_pool_wrapper!(ApiTransactionDbPool);
impl_split_pool_wrapper!(ApiWalletDbPool);
