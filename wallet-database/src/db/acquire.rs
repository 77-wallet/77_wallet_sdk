use anyhow;
use lazy_static;
use sqlx;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::Semaphore, time::timeout};
use tracing::{error, trace};

use crate::db_pool::ApiFundsDbPool;

/// 连接获取超时（秒）
const PERMIT_TIMEOUT: Duration = Duration::from_secs(1);
/// 池获取超时（秒）
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);

// 数据库连接信号量
lazy_static::lazy_static! {
    static ref DB_SEMAPHORE: Arc<Semaphore> = Arc::new(Semaphore::new(10));
}

/// 数据库连接守卫
pub struct DbConnGuard {
    conn: sqlx::pool::PoolConnection<sqlx::Sqlite>,
    pub permit: tokio::sync::OwnedSemaphorePermit,
    pub start_ts: Instant,
    pub checkout_start: Instant,
    pub sql_count: std::sync::atomic::AtomicU32,
}

impl DbConnGuard {
    pub fn conn(&mut self) -> &mut sqlx::pool::PoolConnection<sqlx::Sqlite> {
        &mut self.conn
    }

    pub fn record_query(&self) {
        self.sql_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl std::ops::Deref for DbConnGuard {
    type Target = sqlx::pool::PoolConnection<sqlx::Sqlite>;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl std::ops::DerefMut for DbConnGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

impl Drop for DbConnGuard {
    fn drop(&mut self) {
        let acquire_duration = self.checkout_start.duration_since(self.start_ts);
        let hold_duration = std::time::Instant::now().duration_since(self.checkout_start);
        let sql_count = self.sql_count.load(std::sync::atomic::Ordering::Relaxed);
        trace!(acquire_duration = ?acquire_duration, hold_duration = ?hold_duration, sql_count = sql_count, "Database connection released");
    }
}

/// 统一获取数据库连接
///
/// # Returns
/// - `Ok(DbConnGuard)`: 成功获取连接
/// - `Err(anyhow::Error)`: 获取连接失败
pub async fn acquire_conn(pool: &ApiFundsDbPool) -> anyhow::Result<DbConnGuard> {
    let start_ts = Instant::now();

    // 1. 尝试获取信号量许可（快速失败）
    let permit = match DB_SEMAPHORE.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            // 2. 如果快速失败，尝试带超时获取
            match timeout(PERMIT_TIMEOUT, DB_SEMAPHORE.clone().acquire_owned()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(e)) => {
                    error!(error = %e, "Semaphore acquire error");
                    return Err(anyhow::anyhow!("Semaphore acquire error: {}", e));
                }
                Err(_) => {
                    error!("Semaphore acquire timeout");
                    return Err(anyhow::anyhow!("Semaphore acquire timeout"));
                }
            }
        }
    };

    // 3. 获取数据库连接（带超时）
    let conn = match timeout(POOL_ACQUIRE_TIMEOUT, pool.write_ref().acquire()).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => {
            error!(error = %e, "Pool acquire error");
            return Err(anyhow::anyhow!("Pool acquire error: {}", e));
        }
        Err(_) => {
            error!("Pool acquire timeout");
            return Err(anyhow::anyhow!("Pool acquire timeout"));
        }
    };

    let acquire_duration = start_ts.elapsed();
    trace!(duration = ?acquire_duration, "Database connection acquired");

    let checkout_start = std::time::Instant::now();
    Ok(DbConnGuard {
        conn,
        permit,
        start_ts,
        checkout_start,
        sql_count: std::sync::atomic::AtomicU32::new(0),
    })
}

/// 导出为 acquire，方便使用
pub use acquire_conn as acquire;

#[cfg(test)]
mod tests {
    use super::acquire_conn;
    use crate::{
        SqlitePoolConfig,
        dao::api_nonce::ApiNonceDao,
        repositories::test_helper::{setup_api_funds_pool, setup_api_funds_pool_with_config},
    };
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn acquire_conn_success_returns_guard_and_query_works() {
        let pool = setup_api_funds_pool("wallet_db_acquire_success").await;
        ApiNonceDao::upsert_nonce_exact(
            pool.write_ref(),
            "0xacquire_success",
            wallet_types::constant::chain_code::ETHEREUM,
            9,
        )
        .await
        .unwrap();

        let mut guard = acquire_conn(&pool).await.unwrap();
        let nonce = ApiNonceDao::get_api_nonce(
            guard.conn().as_mut(),
            "0xacquire_success",
            wallet_types::constant::chain_code::ETHEREUM,
        )
        .await
        .unwrap();
        assert_eq!(nonce, 9);
    }

    #[tokio::test]
    async fn acquire_conn_writer_busy_returns_timeout_error() {
        let pool = setup_api_funds_pool("wallet_db_acquire_timeout").await;
        ApiNonceDao::upsert_nonce_exact(
            pool.write_ref(),
            "0xacquire_timeout",
            wallet_types::constant::chain_code::ETHEREUM,
            1,
        )
        .await
        .unwrap();

        let (ready_tx, ready_rx) = oneshot::channel();
        let pool_hold = pool.clone();
        let holder = tokio::spawn(async move {
            let mut tx = pool_hold.write_ref().begin().await.unwrap();
            ApiNonceDao::upsert_nonce_exact(
                tx.as_mut(),
                "0xacquire_timeout",
                wallet_types::constant::chain_code::ETHEREUM,
                2,
            )
            .await
            .unwrap();
            let _ = ready_tx.send(());
            tokio::time::sleep(Duration::from_secs(4)).await;
            tx.commit().await.unwrap();
        });

        ready_rx.await.unwrap();
        let err = match acquire_conn(&pool).await {
            Ok(_) => panic!("writer busy should timeout"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("Pool acquire timeout"), "unexpected error: {err:?}");

        holder.await.unwrap();
    }

    #[tokio::test]
    async fn acquire_conn_permit_exhausted_returns_timeout_error() {
        let cfg = SqlitePoolConfig { reader_max_connections: 4, writer_max_connections: 20 };
        let pool = setup_api_funds_pool_with_config("wallet_db_acquire_permit_timeout", cfg).await;

        let mut guards = Vec::with_capacity(10);
        let mut saw_semaphore_timeout = false;
        for _ in 0..30 {
            match acquire_conn(&pool).await {
                Ok(guard) => guards.push(guard),
                Err(err) => {
                    assert!(
                        err.to_string().contains("Semaphore acquire timeout"),
                        "unexpected error: {err:?}"
                    );
                    saw_semaphore_timeout = true;
                    break;
                }
            }
        }

        assert!(saw_semaphore_timeout, "expected semaphore timeout was not observed");

        drop(guards);
    }
}
