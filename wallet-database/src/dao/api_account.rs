use crate::entities::{
    api_account::{ApiAccountEntity, CreateApiAccountVo},
    api_wallet::ApiWalletType,
};
use sqlx::{Executor, Sqlite};

impl ApiAccountEntity {
    /// 插入多个账户（存在则更新 updated_at）
    pub async fn upsert_multi<'a, E>(
        exec: E,
        reqs: Vec<CreateApiAccountVo>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if reqs.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::<Sqlite>::new(
            "INSERT INTO api_account (
                account_id, name, address, pubkey, private_key, address_type,
                wallet_address, derivation_path, derivation_path_index,
                chain_code, wallet_type, status, is_init, created_at, updated_at
            ) ",
        );

        query_builder.push_values(reqs, |mut b, item| {
            b.push_bind(item.account_id)
                .push_bind(item.name)
                .push_bind(item.address)
                .push_bind(item.pubkey)
                .push_bind(item.private_key)
                .push_bind(item.address_type)
                .push_bind(item.wallet_address)
                .push_bind(item.derivation_path)
                .push_bind(item.derivation_path_index)
                .push_bind(item.chain_code)
                .push_bind(item.wallet_type)
                .push_bind(1)
                .push_bind(0)
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
        });

        query_builder.push(
            " ON CONFLICT(address, chain_code, address_type) DO UPDATE SET
              updated_at = excluded.updated_at",
        );

        let query = query_builder.build();
        query
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 查询某个钱包地址下的所有账户
    pub async fn list_by_wallet<'a, E>(
        exec: E,
        wallet_address: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_account WHERE wallet_address = $1"#;

        sqlx::query_as::<_, Self>(sql)
            .bind(wallet_address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 编辑账户名
    pub async fn edit_name<'a, E>(
        exec: E,
        account_id: i32,
        wallet_address: &str,
        name: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_account SET 
                name = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE wallet_address = $1 AND account_id = $2
            RETURNING *
        "#;

        sqlx::query_as::<_, Self>(sql)
            .bind(wallet_address)
            .bind(account_id)
            .bind(name)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 查询最大 account_id 的账户记录（用于推断下一个 ID）
    pub async fn latest_by_wallet<'a, E>(
        exec: E,
        wallet_address: &str,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_account 
            WHERE wallet_address = $1 
            ORDER BY account_id DESC 
            LIMIT 1
        "#;

        sqlx::query_as::<_, Self>(sql)
            .bind(wallet_address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 重置账户状态
    pub async fn reset_status<'a, E>(
        exec: E,
        wallet_address: &str,
        status: i32,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_account SET 
                status = $2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE wallet_address = $1
            RETURNING *
        "#;

        sqlx::query_as::<_, Self>(sql)
            .bind(wallet_address)
            .bind(status)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 根据 address + chain_code + address_type 精确查找
    pub async fn find_one<'a, E>(
        exec: E,
        address: &str,
        chain_code: &str,
        address_type: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_account 
            WHERE address = $1 AND chain_code = $2 AND address_type = $3 AND wallet_type = $4
        "#;

        sqlx::query_as::<_, Self>(sql)
            .bind(address)
            .bind(chain_code)
            .bind(address_type)
            .bind(api_wallet_type)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn physical_delete<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        DELETE FROM api_account
        WHERE wallet_address = $1 AND account_id = $2
        RETURNING *
        "#;

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(wallet_address)
            .bind(account_id)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn has_account_id<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_account WHERE wallet_address = $1 AND account_id = $2 AND wallet_type = $3
            "#;

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(wallet_address)
            .bind(account_id)
            .bind(api_wallet_type)
            .fetch_optional(exec)
            .await
            .map(|v| v.is_some())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn account_detail_by_max_id_and_wallet_address<'a, E>(
        executor: E,
        wallet_address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_account where wallet_address = $1
            AND wallet_type = $2
                   ORDER BY account_id DESC
                   LIMIT 1;";

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(wallet_address)
            .bind(api_wallet_type)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
