use crate::{
    entities::{
        api_account::{ApiAccountEntity, CreateApiAccountVo},
        api_wallet::ApiWalletType,
    },
    sql_utils::{
        SqlExecutableReturn as _, query_builder::DynamicQueryBuilder,
        update_builder::DynamicUpdateBuilder,
    },
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiAccountDao;

impl ApiAccountDao {
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
                chain_code, wallet_type, status, is_init, is_used, created_at, updated_at
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
                .push_bind(false)
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
        });

        query_builder.push(
            " ON CONFLICT(address, chain_code, address_type) DO UPDATE SET
              updated_at = excluded.updated_at",
        );

        let query = query_builder.build();
        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn lists_by_wallet_address<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("status", 1)
            .and_where_eq_opt("account_id", account_id)
            .and_where_eq_opt("chain_code", chain_code)
            .fetch_all(exec)
            .await
    }

    /// 编辑账户名
    pub async fn edit_name<'a, E>(
        exec: E,
        account_id: i32,
        wallet_address: &str,
        name: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
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

        sqlx::query_as::<_, ApiAccountEntity>(sql)
            .bind(wallet_address)
            .bind(account_id)
            .bind(name)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 标记is_used
    pub async fn update_is_used<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
        chain_code: &str,
        is_used: bool,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_account SET 
                is_used = $3,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE wallet_address = $1 AND account_id = $2 AND chain_code = $4
            RETURNING *
        "#;

        sqlx::query_as::<_, ApiAccountEntity>(sql)
            .bind(wallet_address)
            .bind(account_id)
            .bind(is_used)
            .bind(chain_code)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 查询最大 account_id 的账户记录（用于推断下一个 ID）
    pub async fn latest_by_wallet<'a, E>(
        exec: E,
        wallet_address: &str,
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM api_account 
            WHERE wallet_address = $1 
            ORDER BY account_id DESC 
            LIMIT 1
        "#;

        sqlx::query_as::<_, ApiAccountEntity>(sql)
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
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
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

        sqlx::query_as::<_, ApiAccountEntity>(sql)
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
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("address", address)
            .and_where_eq("chain_code", chain_code)
            .and_where_eq("address_type", address_type)
            .and_where_eq("wallet_type", api_wallet_type)
            .fetch_optional(exec)
            .await
    }

    pub async fn api_account_list<'a, E>(
        executor: E,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_codes: Vec<String>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_in("chain_code", &chain_codes)
            .and_where_eq_opt("wallet_address", wallet_address)
            .and_where_eq_opt("account_id", account_id)
            .fetch_all(executor)
            .await
    }

    pub async fn find_all_by_wallet_address_index<'a, E>(
        exec: E,
        wallet_address: &str,
        chain_code: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicQueryBuilder::new("SELECT * FROM api_account");

        builder
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("chain_code", chain_code)
            .and_where_eq("account_id", account_id)
            .fetch_all(exec)
            .await
    }

    pub async fn physical_delete<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        DELETE FROM api_account
        WHERE wallet_address = $1 AND account_id = $2
        RETURNING *
        "#;

        sqlx::query_as::<sqlx::Sqlite, ApiAccountEntity>(sql)
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

        sqlx::query_as::<sqlx::Sqlite, ApiAccountEntity>(sql)
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
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_account where wallet_address = $1
            AND wallet_type = $2
                   ORDER BY account_id DESC
                   LIMIT 1;";

        sqlx::query_as::<sqlx::Sqlite, ApiAccountEntity>(sql)
            .bind(wallet_address)
            .bind(api_wallet_type)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find_one_by_address_chain_code<'a, E>(
        address: &str,
        chain_code: &str,
        exec: E,
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("address", address)
            .and_where_eq("chain_code", chain_code)
            .fetch_optional(exec)
            .await
    }

    pub async fn get_all_account_indices<'a, E>(
        exec: E,
        wallet_address: &str,
    ) -> Result<Vec<u32>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT DISTINCT account_id FROM api_account")
            .and_where_eq("wallet_type", ApiWalletType::SubAccount)
            .and_where_eq("wallet_address", wallet_address)
            .order_by("account_id")
            .fetch_all(exec)
            .await
            .map(|rows: Vec<(u32,)>| rows.into_iter().map(|(id,)| id).collect())
    }

    pub async fn init<'a, E>(
        exec: E,
        address: &str,
        chain_code: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicUpdateBuilder::new("api_account")
            .set("is_init", 1)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", address)
            .and_where_eq("chain_code", chain_code)
            .fetch_all(exec)
            .await
    }

    pub async fn expand<'a, E>(
        exec: E,
        address: &str,
        chain_code: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicUpdateBuilder::new("api_account")
            .set("is_expand", 1)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", address)
            .and_where_eq("chain_code", chain_code)
            .fetch_all(exec)
            .await
    }
}
