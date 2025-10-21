use crate::{
    entities::api_wallet::{ApiWalletEntity, ApiWalletType},
    sql_utils::{
        SqlExecutableNoReturn, SqlExecutableReturn as _, query_builder::DynamicQueryBuilder,
        update_builder::DynamicUpdateBuilder,
    },
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWalletDao;

impl ApiWalletDao {
    pub async fn upsert<'a, E>(
        exec: E,
        address: &str,
        uid: &str,
        name: &str,
        phrase: &str,
        seed: &str,
        status: u8,
        api_wallet_type: ApiWalletType,
        binding_address: Option<&str>,
        // merchant_id: &str,
        // app_id: &str,
    ) -> Result<ApiWalletEntity, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_wallet (
                address, uid, name, phrase, seed,
                status, is_init, api_wallet_type, binding_address,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(address)
            DO UPDATE SET
                uid = excluded.uid,
                name = excluded.name,
                phrase = excluded.phrase,
                seed = excluded.seed,
                status = excluded.status,
                api_wallet_type = excluded.api_wallet_type,
                binding_address = excluded.binding_address,
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
            .bind(api_wallet_type)
            .bind(binding_address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.pop().ok_or(crate::DatabaseError::ReturningNone)?)
    }

    pub async fn detail<'a, E>(
        exec: E,
        address: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_wallet WHERE address = ? AND status = 1;";
        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn wallet_latest<'a, E>(exec: E) -> Result<Option<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_wallet WHERE status = 1
                   ORDER BY updated_at DESC
                   LIMIT 1;";

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn uid_list<'a, E>(exec: E) -> Result<Vec<(String,)>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT DISTINCT uid FROM api_wallet WHERE status = 1;";

        sqlx::query_as::<sqlx::Sqlite, (String,)>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail_by_uid<'a, E>(
        exec: E,
        uid: &str,
    ) -> Result<Option<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_wallet")
            .and_where_eq("uid", uid)
            .and_where_eq("status", "1")
            .fetch_optional(exec)
            .await
    }

    pub async fn list<'a, E>(
        exec: E,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM api_wallet".to_string();
        let mut conditions = Vec::new();

        if api_wallet_type.is_some() {
            conditions.push("api_wallet_type = ?".to_string());
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query_as::<_, ApiWalletEntity>(&sql);

        if let Some(api_wallet_type) = api_wallet_type {
            query = query.bind(api_wallet_type);
        }

        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_merchain_id<'a, E>(
        exec: E,
        address: &str,
        merchant_id: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                merchant_id = $1,
                updated_at = $2
            WHERE address = $3 AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(merchant_id)
            .bind(now)
            .bind(address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn bind_withdraw_and_subaccount_relation<'a, E>(
        exec: E,
        wallet_address: &str,
        binding_address: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicUpdateBuilder::new("api_wallet")
            .set("binding_address", binding_address)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", wallet_address);
        SqlExecutableNoReturn::execute(&builder, exec).await
    }

    pub async fn update_app_id<'a, E>(
        exec: E,
        address: &str,
        app_id: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                app_id = $1,
                updated_at = $2
            WHERE address = $3 AND status = 1
            RETURNING *;
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiWalletEntity>(sql)
            .bind(app_id)
            .bind(now)
            .bind(address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn unbind_uid<'a, E>(
        exec: E,
        address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let now = sqlx::types::chrono::Utc::now();
        let sql = r#"
            UPDATE api_wallet SET
                app_id = null,
                merchant_id = null,
                updated_at = $1
            WHERE address = $2  AND api_wallet_type = $3 AND status = 1
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
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
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

    pub async fn mark_init<'a, E>(exec: E, uid: &str) -> Result<ApiWalletEntity, crate::Error>
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

    pub async fn physical_delete<'a, E>(
        exec: E,
        addresses: &[&str],
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
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

    pub async fn reset_status<'a, E>(
        exec: E,
        address: &str,
    ) -> Result<Vec<ApiWalletEntity>, crate::Error>
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
