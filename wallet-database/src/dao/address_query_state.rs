use sqlx::{Executor, Sqlite};

use crate::entities::address_query_state::{
    AddressQueryStateEntity, AddressQueryStatus, CreateAddressQueryStateEntity,
};

pub struct AddressQueryStateDao {}

impl AddressQueryStateDao {
    pub async fn upsert<'a, E>(
        exec: E,
        req: CreateAddressQueryStateEntity,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql =
            "INSERT INTO address_query_state (uid, chain_code, status, last_page, total_remote, created_at, updated_at)
            VALUES
            (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT (uid, chain_code) DO UPDATE SET
                status = excluded.status,
                last_page = excluded.last_page,
                total_remote = CASE WHEN excluded.total_remote > 0 AND total_remote = 0 THEN excluded.total_remote ELSE total_remote END,
                updated_at = excluded.updated_at";

        sqlx::query(sql)
            .bind(req.uid)
            .bind(req.chain_code)
            .bind(req.status)
            .bind(req.last_page)
            .bind(req.total_remote)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_by_uid_and_chain<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<Option<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM address_query_state WHERE uid = ? AND chain_code = ?";

        sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .bind(uid)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_status<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        status: AddressQueryStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "UPDATE address_query_state SET status = ?,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                        WHERE uid = ? AND chain_code = ?";

        sqlx::query(sql)
            .bind(status)
            .bind(uid)
            .bind(chain_code)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete<'a, E>(exec: E, uid: &str, chain_code: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "DELETE FROM address_query_state WHERE uid = ? AND chain_code = ?";

        sqlx::query(sql)
            .bind(uid)
            .bind(chain_code)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_by_uid<'a, E>(exec: E, uid: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "DELETE FROM address_query_state WHERE uid = ?";

        sqlx::query(sql)
            .bind(uid)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_all<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "DELETE FROM address_query_state";

        sqlx::query(sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_by_uid<'a, E>(
        exec: E,
        uid: &str,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM address_query_state WHERE uid = ? ORDER BY created_at DESC";

        sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .bind(uid)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_by_status<'a, E>(
        exec: E,
        status: AddressQueryStatus,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM address_query_state WHERE status = ? ORDER BY created_at ASC";

        sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .bind(status)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取需要恢复的任务（Failed + 长时间未更新的Running）
    /// 长时间指：updated_at < now - 10 minutes
    pub async fn list_recoverable_tasks<'a, E>(
        exec: E,
        include_stuck_running: bool,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = if include_stuck_running {
            "SELECT * FROM address_query_state 
             WHERE status = ? OR (status = ? AND updated_at < datetime('now', '-10 minutes')) 
             ORDER BY created_at ASC"
        } else {
            "SELECT * FROM address_query_state 
             WHERE status = ? 
             ORDER BY created_at ASC"
        };

        sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .bind(AddressQueryStatus::Failed)
            .bind(AddressQueryStatus::Running)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_running_by_uid<'a, E>(
        exec: E,
        uid: &str,
    ) -> Result<Vec<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM address_query_state WHERE uid = ? AND status = 1 ORDER BY created_at ASC";

        sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .bind(uid)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn is_running<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT EXISTS(
            SELECT 1
            FROM address_query_state
            WHERE uid = ?
            AND chain_code = ?
            AND status = 1
        )";

        let exists: i64 = sqlx::query_scalar(sql)
            .bind(uid)
            .bind(chain_code)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(exists == 1)
    }

    pub async fn count_by_status<'a, E>(
        exec: E,
        status: AddressQueryStatus,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT COUNT(*) FROM address_query_state WHERE status = ?";

        let count: i64 = sqlx::query_scalar(sql)
            .bind(status)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(count)
    }

    /// 获取所有地址查询状态
    pub async fn get_all<'a, E>(exec: E) -> Result<Vec<AddressQueryStateEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM address_query_state";

        let states = sqlx::query_as::<sqlx::Sqlite, AddressQueryStateEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(states)
    }

    /// 更新最后处理的页码
    pub async fn update_last_page<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        last_page: i64,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "UPDATE address_query_state SET 
            last_page = ?, 
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE uid = ? AND chain_code = ?";

        sqlx::query(sql)
            .bind(last_page)
            .bind(uid)
            .bind(chain_code)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 更新总远程地址数
    pub async fn update_total_remote<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        total_remote: i64,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "UPDATE address_query_state SET 
            total_remote = CASE WHEN ? > 0 AND total_remote = 0 THEN ? ELSE total_remote END,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE uid = ? AND chain_code = ?";

        sqlx::query(sql)
            .bind(total_remote)
            .bind(total_remote)
            .bind(uid)
            .bind(chain_code)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
