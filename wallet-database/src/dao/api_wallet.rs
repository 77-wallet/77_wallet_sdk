use crate::entities::api_wallet::{ApiWalletEntity, ApiWalletType};
use sqlx::{Executor, Sqlite};

impl ApiWalletEntity {
    pub async fn upsert<'a, E>(
        exec: E,
        address: &str,
        uid: &str,
        name: &str,
        phrase: &str,
        seed: &str,
        status: u8,
        wallet_type: ApiWalletType,
        // merchant_id: &str,
        // app_id: &str,
    ) -> Result<Self, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_wallet (
                address, uid, name, phrase, seed,
                status, is_init, wallet_type,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, 0, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(address)
            DO UPDATE SET
                uid = excluded.uid,
                name = excluded.name,
                phrase = excluded.phrase,
                seed = excluded.seed,
                status = excluded.status,
                wallet_type = excluded.wallet_type,
                updated_at = excluded.updated_at
            RETURNING *;
        "#;

        let mut res = sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(address)
            .bind(uid)
            .bind(name)
            .bind(phrase)
            .bind(seed)
            .bind(status)
            .bind(wallet_type)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.pop().ok_or(crate::DatabaseError::ReturningNone)?)
    }

    pub async fn detail<'a, E>(
        exec: E,
        address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_wallet WHERE address = ? AND wallet_type = ? AND status = 1;";
        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(address)
            .bind(api_wallet_type)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail_by_uid<'a, E>(
        exec: E,
        uid: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_wallet WHERE uid = ? AND wallet_type = ? AND status = 1;";
        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(uid)
            .bind(api_wallet_type)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list<'a, E>(
        exec: E,
        address: Option<&str>,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM api_wallet".to_string();
        let mut conditions = Vec::new();

        if address.is_some() {
            conditions.push("address = ?".to_string());
        }

        if api_wallet_type.is_some() {
            conditions.push("wallet_type = ?".to_string());
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query_as::<_, Self>(&sql);

        if let Some(address) = address {
            query = query.bind(address);
        }
        if let Some(api_wallet_type) = api_wallet_type {
            query = query.bind(api_wallet_type);
        }

        query
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_merchain_id<'a, E>(
        exec: E,
        address: &str,
        merchant_id: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                merchant_id = $1,
                updated_at = $2
            WHERE address = $3 AND wallet_type = $4 AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(merchant_id)
            .bind(now)
            .bind(address)
            .bind(api_wallet_type)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_app_id<'a, E>(
        exec: E,
        address: &str,
        app_id: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                app_id = $1,
                updated_at = $2
            WHERE address = $3  AND wallet_type = $4 AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(app_id)
            .bind(now)
            .bind(address)
            .bind(api_wallet_type)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn unbind_uid<'a, E>(
        exec: E,
        address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                app_id = null,
                merchant_id = null,
                updated_at = $1
            WHERE address = $2  AND wallet_type = $3 AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(now)
            .bind(address)
            .bind(api_wallet_type)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn edit_name<'a, E>(
        exec: E,
        address: &str,
        name: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                name = ?,
                updated_at = ?
            WHERE address = ? AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(name)
            .bind(now)
            .bind(address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_init<'a, E>(exec: E, uid: &str) -> Result<Self, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                is_init = 1,
                updated_at = ?
            WHERE uid = ? AND status = 1
            RETURNING *;
        "#;

        let mut res = sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(now)
            .bind(uid)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.pop().ok_or(crate::DatabaseError::ReturningNone)?)
    }

    pub async fn delete_by_address<'a, E>(
        exec: E,
        addresses: &[&str],
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let condition = crate::any_in_collection(addresses, "','");
        let sql = format!(
            r#"
            DELETE FROM api_wallet
            WHERE address IN ('{}')
            RETURNING *;
            "#,
            condition
        );

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn reset_status<'a, E>(exec: E, address: &str) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_wallet SET
                status = 2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE address = ?
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
