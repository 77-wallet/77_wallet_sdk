use crate::entities::api_business_notification::{
    ApiBusinessNotification, NewApiBusinessNotification,
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiBusinessNotificationDao;
impl ApiBusinessNotificationDao {
    // EXISTS avoids multiplying messages when several wallet rows share an owner.
    const WITHDRAWAL_SCOPE: &'static str = "EXISTS (
        SELECT 1 FROM api_wallet w WHERE w.uid = api_business_notification.uid
        AND w.status = 1 AND w.api_wallet_type = 2 AND TRIM(w.uid) <> '')";

    pub async fn count_for_withdrawal_wallets<'a, E>(
        exec: E,
        unresolved_only: bool,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_scalar(&format!("SELECT count(*) FROM api_business_notification WHERE {} AND (? = 0 OR resolved_at IS NULL)", Self::WITHDRAWAL_SCOPE))
            .bind(unresolved_only).fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_for_withdrawal_wallets<'a, E>(
        exec: E,
        unresolved_only: bool,
        page_size: i64,
        offset: i64,
    ) -> Result<Vec<ApiBusinessNotification>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as(&format!("SELECT * FROM api_business_notification WHERE {} AND (? = 0 OR resolved_at IS NULL) ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?", Self::WITHDRAWAL_SCOPE))
            .bind(unresolved_only).bind(page_size).bind(offset).fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn unread_count_for_withdrawal_wallets<'a, E>(exec: E) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_scalar(&format!(
            "SELECT count(*) FROM api_business_notification WHERE {} AND is_read = 0",
            Self::WITHDRAWAL_SCOPE
        ))
        .fetch_one(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_read_for_withdrawal_wallets<'a, E>(
        exec: E,
        id: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(&format!("UPDATE api_business_notification SET is_read = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE {} AND id = ? AND is_read = 0", Self::WITHDRAWAL_SCOPE))
            .bind(id).execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_all_read_for_withdrawal_wallets<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(&format!("UPDATE api_business_notification SET is_read = 1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE {} AND is_read = 0", Self::WITHDRAWAL_SCOPE))
            .execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find<'a, E>(
        exec: E,
        uid: &str,
        id: &str,
    ) -> Result<Option<ApiBusinessNotification>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_as("SELECT * FROM api_business_notification WHERE uid = ? AND id = ?")
            .bind(uid)
            .bind(id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
    /// Exact durable-event replay preserves resolution; a new payload reopens the alert.
    pub async fn ingest<'a, E>(
        exec: E,
        n: NewApiBusinessNotification<'_>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "INSERT INTO api_business_notification (id, uid, kind, business_key, payload)
            VALUES (?, ?, ?, ?, ?) ON CONFLICT (uid, kind, business_key) DO UPDATE SET
            resolved_at = CASE WHEN api_business_notification.payload = excluded.payload
                THEN api_business_notification.resolved_at ELSE NULL END,
            payload = excluded.payload, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .bind(n.id)
        .bind(n.uid)
        .bind(n.kind)
        .bind(n.business_key)
        .bind(n.payload)
        .execute(exec)
        .await
        .map(|_| ())
        .map_err(|e| crate::Error::Database(e.into()))
    }
    pub async fn upsert<'a, E>(
        exec: E,
        notification: NewApiBusinessNotification<'_>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "INSERT INTO api_business_notification (id, uid, kind, business_key, payload)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (uid, kind, business_key) DO UPDATE SET
               payload = excluded.payload,
               updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .bind(notification.id)
        .bind(notification.uid)
        .bind(notification.kind)
        .bind(notification.business_key)
        .bind(notification.payload)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn count<'a, E>(
        exec: E,
        uid: &str,
        unresolved_only: bool,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let total_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM api_business_notification
             WHERE uid = ? AND (? = 0 OR resolved_at IS NULL)",
        )
        .bind(uid)
        .bind(unresolved_only)
        .fetch_one(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(total_count)
    }

    pub async fn list<'a, E>(
        exec: E,
        uid: &str,
        unresolved_only: bool,
        page_size: i64,
        offset: i64,
    ) -> Result<Vec<ApiBusinessNotification>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let data = sqlx::query_as::<_, ApiBusinessNotification>(
            "SELECT * FROM api_business_notification
             WHERE uid = ? AND (? = 0 OR resolved_at IS NULL)
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(uid)
        .bind(unresolved_only)
        .bind(page_size)
        .bind(offset)
        .fetch_all(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(data)
    }

    pub async fn unread_count<'a, E>(exec: E, uid: &str) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query_scalar(
            "SELECT count(*) FROM api_business_notification WHERE uid = ? AND is_read = 0",
        )
        .bind(uid)
        .fetch_one(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_all_read<'a, E>(exec: E, uid: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "UPDATE api_business_notification SET is_read = 1,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE uid = ? AND is_read = 0",
        )
        .bind(uid)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn mark_read<'a, E>(exec: E, uid: &str, id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "UPDATE api_business_notification SET is_read = 1,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE uid = ? AND id = ?",
        )
        .bind(uid)
        .bind(id)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn mark_resolved<'a, E>(
        exec: E,
        uid: &str,
        id: &str,
        resolved: bool,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "UPDATE api_business_notification SET
             resolved_at = CASE WHEN ? THEN strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ELSE NULL END,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE uid = ? AND id = ?",
        )
        .bind(resolved)
        .bind(uid)
        .bind(id)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn resolve_if_payload_matches<'a, E>(
        exec: E,
        uid: &str,
        id: &str,
        payload: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        sqlx::query(
            "UPDATE api_business_notification SET
             resolved_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE uid = ? AND id = ? AND payload = ? AND resolved_at IS NULL",
        )
        .bind(uid)
        .bind(id)
        .bind(payload)
        .execute(exec)
        .await
        .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }
}
