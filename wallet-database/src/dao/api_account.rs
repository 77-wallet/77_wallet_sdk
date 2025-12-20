use crate::{
    entities::{
        account::AccountEntity,
        api_account::{
            AccountToWalletAddress, ApiAccountEntity, ApiAccountWalletMapping, CreateApiAccountVo,
        },
        api_wallet::ApiWalletType,
    },
    sql_utils::{
        SqlExecutableNoReturn, SqlExecutableReturn as _, query_builder::DynamicQueryBuilder,
        update_builder::DynamicUpdateBuilder,
    },
};
use sqlx::{Executor, Row, Sqlite, sqlite::SqliteRow};
use wallet_types::chain::address::category::AddressCategory;

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
                chain_code, api_wallet_type, status, is_init, is_used, created_at, updated_at
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
                .push_bind(item.api_wallet_type)
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

    pub async fn physical_delete_all<'a, E>(
        exec: E,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // use crate::sql_utils::SqlExecutableReturn;
        crate::sql_utils::delete_builder::DynamicDeleteBuilder::new("api_account")
            .and_where_in("wallet_address", wallet_addresses)
            .fetch_all(exec)
            .await
    }

    pub async fn count_unique_account_ids<'a, E>(
        exec: E,
        wallet_address: &str,
    ) -> Result<u32, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT COUNT(DISTINCT account_id) as count FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .fetch_one(exec)
            .await
            .map(|(count,)| count)
    }

    pub async fn physical_delete<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        crate::sql_utils::delete_builder::DynamicDeleteBuilder::new("api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("account_id", account_id)
            .fetch_all(exec)
            .await
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

    pub async fn list_inited_indices<'a, E>(
        exec: E,
        wallet_address: &str,
        chain_code: &str,
    ) -> Result<Vec<(i32,)>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT DISTINCT derivation_path_index FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("chain_code", chain_code)
            .and_where_eq("is_init", 1)
            .fetch_all(exec)
            .await
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
            .and_where_eq("api_wallet_type", api_wallet_type)
            .fetch_optional(exec)
            .await
    }

    pub async fn api_account_list<'a, E>(
        executor: E,
        wallet_address: Option<String>,
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

    pub async fn has_account_id<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("account_id", account_id)
            .and_where_eq("api_wallet_type", api_wallet_type)
            .fetch_optional(exec)
            .await
            .map(|v: Option<ApiAccountEntity>| v.is_some())
    }

    pub async fn account_detail_by_max_id_and_wallet_address<'a, E>(
        executor: E,
        wallet_address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("api_wallet_type", api_wallet_type)
            .order_by("account_id DESC")
            .limit(1)
            .fetch_optional(executor)
            .await
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
            .and_where_eq("status", 1)
            .fetch_optional(exec)
            .await
    }

    pub async fn find_one_by_address<'a, E>(
        address: &str,
        exec: E,
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("address", address)
            .and_where_eq("status", 1)
            .fetch_optional(exec)
            .await
    }

    /// 批量查询账户（通过地址列表）
    pub async fn find_by_addresses<'a, E>(
        addresses: &[String],
        exec: E,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }

        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_in("address", addresses)
            .and_where_eq("status", 1)
            .fetch_all(exec)
            .await
    }

    pub async fn find_one_by_wallet_address_account_id_chain_code<'a, E>(
        wallet_address: &str,
        account_id: u32,
        chain_code: &str,
        exec: E,
    ) -> Result<Option<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("account_id", account_id)
            .and_where_eq("chain_code", chain_code)
            .fetch_optional(exec)
            .await
    }

    pub async fn get_all_account_indices<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<u32>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new(
            "SELECT DISTINCT account_id FROM api_account
            LEFT JOIN api_wallet ON
               api_account.wallet_address = api_wallet.address
        ",
        )
        .and_where_eq("api_account.api_wallet_type", ApiWalletType::SubAccount)
        .and_where_eq("api_wallet.uid", uid)
        .and_where_eq("chain_code", chain_code)
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

    pub async fn account_list<'a, E>(
        executor: E,
        wallet_address: Option<&str>,
        address: Option<&str>,
        derivation_path: Option<&str>,
        chain_codes: Vec<String>,
        account_id: Option<u32>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM api_account")
            .and_where_in("chain_code", &chain_codes)
            .and_where_eq_opt("wallet_address", wallet_address)
            .and_where_eq_opt("address", address)
            .and_where_eq_opt("derivation_path", derivation_path)
            .and_where_eq_opt("account_id", account_id)
            .fetch_all(executor)
            .await
    }

    pub async fn account_to_wallet<'a, E>(
        executor: E,
    ) -> Result<Vec<AccountToWalletAddress>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT address, wallet_address FROM api_account")
            .fetch_all(executor)
            .await
    }

    pub async fn account_wallet_mapping<'a, E>(
        executor: E,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiAccountWalletMapping>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // let sql = r#"
        //     SELECT DISTINCT
        //         api_account.account_id,
        //         api_account.name,
        //         api_account.address,
        //         api_account.wallet_address,
        //         api_wallet.uid
        //     FROM
        //         api_account
        //     LEFT JOIN
        //         api_wallet
        //     ON
        //         api_account.wallet_address = api_wallet.address;
        //     "#;
        // sqlx::query_as::<sqlx::Sqlite, AccountWalletMapping>(sql)
        //     .fetch_all(executor)
        //     .await
        //     .map_err(|e| crate::Error::Database(e.into()))

        DynamicQueryBuilder::new(
            "SELECT DISTINCT 
                api_account.account_id,
                api_account.name,
                api_account.address,
                api_account.wallet_address,
                api_wallet.uid,
                api_wallet.seed,
                api_wallet.api_wallet_type
            FROM 
                api_account
            LEFT JOIN 
                api_wallet
            ON 
                api_account.wallet_address = api_wallet.address
            ",
        )
        .and_where_eq_opt("api_wallet.api_wallet_type", api_wallet_type)
        .fetch_all(executor)
        .await
    }

    pub async fn edit_account_name<'a, E>(
        executor: E,
        wallet_address: &str,
        account_id: u32,
        name: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicUpdateBuilder::new("api_account")
            .set("name", name)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("wallet_address", wallet_address)
            .and_where_eq("account_id", account_id)
            .fetch_all(executor)
            .await
    }

    pub async fn update_private_key<'a, E>(
        executor: E,
        address: &str,
        private_key: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicUpdateBuilder::new("api_account")
            .set("private_key", private_key)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", address);
        SqlExecutableNoReturn::execute(&builder, executor).await
    }

    pub async fn lists_by_wallet_address_v2<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiAccountSummeryEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let limit = page_size;
        let mut offset = page_size * (page - 1);
        if offset < 0 {
            offset = 0;
        }

        let account_id_sql = if let Some(account_id) = account_id {
            format!("AND api_account.account_id = '{account_id}'")
        } else {
            "".to_string()
        };

        let chain_code_sql = if let Some(chain_code) = chain_code {
            format!("AND api_account.chain_code = '{chain_code}'")
        } else {
            "".to_string()
        };

        let sql = format!(
            r#"
select * from(
SELECT
all_data.account_id 				                    AS account_id,
all_data.name 							                AS account_name,
all_data.api_wallet_type 		                        AS api_wallet_type,
all_data.wallet_address 		                        AS wallet_address,
CAST(SUM(all_data.total_coin_quantity) AS REAL) 		AS total_coins_quantity,
CAST(all_data.coin_unit_price AS REAL) AS coin_unit_price,
CAST(SUM(total_coin_amount) AS REAL)			AS total_account_amount,
JSON_GROUP_ARRAY(
    JSON_OBJECT(
        'account_address', all_data.address,
        'wallet_address', all_data.wallet_address,
        'derivation_path', all_data.derivation_path,
				'chain_code', all_data.chain_code,
				'chain_name', all_data.chain_name,
				'address_type', all_data.address_type,
				'coin_name', all_data.coin_name,
				'created_at', all_data.created_at,
				'updated_at', all_data.updated_at
    )
) AS chain_info_list
FROM
(
SELECT
api_account.account_id,api_account.name,api_account.address,api_account.derivation_path,api_account.address_type,
api_account.api_wallet_type,api_account.wallet_address,api_account.address, api_account.chain_code,api_account.created_at,
api_account.updated_at,
api_assets.token_address,api_assets.balance,
api_chain.name 														AS chain_name,
api_coin.price 														AS coin_unit_price,
api_coin.name 														AS coin_name,
SUM(api_assets.balance)  									AS total_coin_quantity,
api_coin.price * SUM(api_assets.balance)  AS total_coin_amount
FROM  api_assets
LEFT JOIN api_account
ON api_assets.address = api_account.address AND api_account.chain_code = api_assets.chain_code
LEFT JOIN api_coin
ON api_coin.chain_code=api_assets.chain_code AND api_coin.token_address=api_assets.token_address
LEFT JOIN  api_chain
ON api_chain.chain_code=api_assets.chain_code
WHERE api_chain.status =1
AND api_account.wallet_address = '{wallet_address}'
 {account_id_sql}
 {chain_code_sql}
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
ORDER BY total_coin_quantity DESC
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id
) as all_datas
ORDER BY total_account_amount DESC
LIMIT $2 OFFSET $3
        "#
        );

        sqlx::query_as::<_, ApiAccountSummeryEntity>(sql.as_str())
            .bind(wallet_address)
            .bind(limit)
            .bind(offset)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn count_by_wallet_address_v2<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let account_id_sql = if let Some(account_id) = account_id {
            format!("AND api_account.account_id = '{account_id}'")
        } else {
            "".to_string()
        };

        let chain_code_sql = if let Some(chain_code) = chain_code {
            format!("AND api_account.chain_code = '{chain_code}'")
        } else {
            "".to_string()
        };

        let sql = format!(
            r#"
SELECT
count(1) as total_count
FROM
(
SELECT
all_data.account_id 				AS account_id,
all_data.name 							AS account_name,
all_data.api_wallet_type 		AS api_wallet_type,
all_data.wallet_address 		AS wallet_address,
CAST(SUM(all_data.total_coin_quantity) AS REAL) 		AS total_coins_quantity,
CAST(all_data.coin_unit_price AS REAL) AS coin_unit_price,
CAST(SUM(total_coin_amount) AS REAL)			AS total_account_amount
FROM
(
SELECT
api_account.account_id,api_account.name,api_account.api_wallet_type,api_account.wallet_address,api_account.address, api_account.chain_code,
api_assets.token_address,api_assets.balance,
api_chain.name 														AS api_chain_name,
api_coin.price 														AS coin_unit_price,
SUM(api_assets.balance)  									AS total_coin_quantity,
api_coin.price * SUM(api_assets.balance)  AS total_coin_amount
FROM api_assets
LEFT JOIN api_account
ON api_assets.address = api_account.address AND api_account.chain_code = api_assets.chain_code
LEFT JOIN api_coin
ON api_coin.chain_code=api_assets.chain_code AND api_coin.token_address=api_assets.token_address
LEFT JOIN api_chain
ON api_chain.chain_code=api_assets.chain_code
WHERE api_chain.status =1
AND api_account.wallet_address = '{wallet_address}'
 {account_id_sql}
 {chain_code_sql}
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id
)AS all_data2
        "#
        );

        #[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
        struct CountResult {
            total_count: i64,
        }

        sqlx::query_as::<_, CountResult>(sql.as_str())
            .bind(wallet_address)
            .fetch_one(exec)
            .await
            .map(|o| o.total_count)
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
// all_data.account_id 				        AS account_id,
// all_data.name 							AS account_name,
// all_data.api_wallet_type 		        AS api_wallet_type,
// all_data.wallet_address 		            AS wallet_address,
// SUM(all_data.total_coin_quantity) 		AS total_coins_quantity,
// all_data.coin_unit_price 		        AS coin_unit_price,
// SUM(total_coin_amount)			        AS total_account_amount
#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiAccountSummeryEntity {
    pub account_id: u32,
    pub account_name: String,
    pub api_wallet_type: ApiWalletType,
    pub coin_unit_price: Option<f64>,
    pub total_coins_quantity: f64,
    pub total_account_amount: Option<f64>,
    pub chain_info_list: serde_json::Value,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ChainInfoEntity {
    pub account_address: String,
    pub wallet_address: String,
    pub derivation_path: String,
    pub chain_code: String,
    pub chain_name: Option<String>,
    pub coin_name: Option<String>,
    pub address_type: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
impl ChainInfoEntity {
    pub fn address_type(&self) -> Option<String> {
        (!self.address_type.is_empty()).then(|| self.address_type.clone())
    }
}

impl ApiAccountSummeryEntity {
    pub fn get_chain_info_list(&self) -> Result<Vec<ChainInfoEntity>, crate::Error> {
        if let serde_json::Value::Array(arr) = &self.chain_info_list {
            let mut result: Vec<ChainInfoEntity> = Vec::new();
            for item in arr {
                let chain_info: Result<ChainInfoEntity, _> = serde_json::from_value(item.clone());

                match chain_info {
                    Ok(info) => {
                        result.push(info);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to deserialize chain info: {}", e)
                    }
                }
            }
            Ok(result)
        } else {
            Ok(vec![])
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountEntitySummer {
    pub account_id: u32,
}

impl ApiAccountDao {
    pub async fn lists_acc_by_wallet_address_v3<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiAccountEntitySummer>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let limit = page_size;
        let mut offset = page_size * (page - 1);
        if offset < 0 {
            offset = 0;
        }

        let account_id_sql = if let Some(account_id) = account_id {
            format!("AND api_account.account_id = '{account_id}'")
        } else {
            "".to_string()
        };

        let chain_code_sql = if let Some(chain_code) = chain_code {
            format!("AND api_account.chain_code = '{chain_code}'")
        } else {
            "".to_string()
        };

        let sql = format!(
            r#"
select account_id from
api_account
WHERE api_account.wallet_address = '{wallet_address}'
 {account_id_sql}
 {chain_code_sql}
GROUP BY api_account.account_id
ORDER BY api_account.account_id ASC
LIMIT {limit} OFFSET {offset}
        "#
        );

        sqlx::query_as::<_, ApiAccountEntitySummer>(sql.as_str())
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn lists_by_wallet_address_v3<'a, E>(
        exec: E,
        wallet_address: &str,
        account_ids: Vec<u32>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiAccountSummeryEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if account_ids.is_empty() {
            return Ok(vec![]);
        }
        let account_ids_str =
            account_ids.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(",");

        let account_ids_sql = format!("AND api_account.account_id in ({account_ids_str})");

        let chain_code_sql = if let Some(chain_code) = chain_code {
            format!("AND api_account.chain_code = '{chain_code}'")
        } else {
            "".to_string()
        };

        let sql = format!(
            r#"
select * from(
SELECT
all_data.account_id 				                    AS account_id,
all_data.name 							                AS account_name,
all_data.api_wallet_type 		                        AS api_wallet_type,
all_data.wallet_address 		                        AS wallet_address,
CAST(SUM(all_data.total_coin_quantity) AS REAL) 		AS total_coins_quantity,
CAST(all_data.coin_unit_price AS REAL) AS coin_unit_price,
CAST(SUM(total_coin_amount) AS REAL)			AS total_account_amount,
JSON_GROUP_ARRAY(
    JSON_OBJECT(
        'account_address', all_data.address,
        'wallet_address', all_data.wallet_address,
        'derivation_path', all_data.derivation_path,
				'chain_code', all_data.chain_code,
				'chain_name', all_data.chain_name,
				'address_type', all_data.address_type,
				'coin_name', all_data.coin_name,
				'created_at', all_data.created_at,
				'updated_at', all_data.updated_at
    )
) AS chain_info_list
FROM
(
SELECT
api_account.account_id,api_account.name,api_account.address,api_account.derivation_path,api_account.address_type,
api_account.api_wallet_type,api_account.wallet_address,api_account.address, api_account.chain_code,api_account.created_at,
api_account.updated_at,
api_assets.token_address,api_assets.balance,
api_chain.name 														AS chain_name,
api_coin.price 														AS coin_unit_price,
api_coin.name 														AS coin_name,
SUM(api_assets.balance)  									AS total_coin_quantity,
api_coin.price * SUM(api_assets.balance)  AS total_coin_amount
FROM  api_assets
LEFT JOIN api_account
ON api_assets.address = api_account.address AND api_account.chain_code = api_assets.chain_code
LEFT JOIN api_coin
ON api_coin.chain_code=api_assets.chain_code AND api_coin.token_address=api_assets.token_address
LEFT JOIN  api_chain
ON api_chain.chain_code=api_assets.chain_code
WHERE api_chain.status =1
AND api_account.wallet_address = '{wallet_address}'
 {account_ids_sql}
 {chain_code_sql}
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
ORDER BY total_coin_quantity DESC
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id
) as all_datas
ORDER BY total_account_amount DESC
        "#
        );
        sqlx::query_as::<_, ApiAccountSummeryEntity>(sql.as_str())
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn count_by_wallet_address_v3<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let account_id_sql = if let Some(account_id) = account_id {
            format!("AND api_account.account_id = '{account_id}'")
        } else {
            "".to_string()
        };

        let chain_code_sql = if let Some(chain_code) = chain_code {
            format!("AND api_account.chain_code = '{chain_code}'")
        } else {
            "".to_string()
        };

        let sql = format!(
            r#"
SELECT
count(1) as total_count
FROM
(
select account_id from
api_account
WHERE api_account.wallet_address = '{wallet_address}'
 {account_id_sql}
 {chain_code_sql}
GROUP BY api_account.account_id
) as all_data
        "#
        );
        #[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
        struct CountResult {
            total_count: i64,
        }

        sqlx::query_as::<_, CountResult>(sql.as_str())
            .fetch_one(exec)
            .await
            .map(|o| o.total_count)
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
