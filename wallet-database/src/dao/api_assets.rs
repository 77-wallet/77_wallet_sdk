use crate::{
    entities::{
        api_assets::{ApiAssetsEntity, ApiAssetsEntityWithAddressType, AssetWithWalletAddress},
        assets::AssetsIdVo,
    },
    error::DatabaseError,
    sql_utils::{
        SqlExecutableNoReturn, SqlExecutableReturn as _, query_builder::DynamicQueryBuilder,
        update_builder::DynamicUpdateBuilder,
    },
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    dao::api_account::ApiAccountSummeryEntity,
    entities::{api_assets::ApiCreateAssetsVo, api_wallet::ApiWalletType},
};
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiAssetsDao;

impl ApiAssetsDao {
    pub async fn list<'a, E>(
        exec: E,
        addr: Vec<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = String::from("SELECT * FROM api_assets");
        let mut conditions = Vec::new();
        conditions.push(" status = 1".to_string());
        conditions.push(
            " EXISTS (
                    SELECT 1
                    FROM api_chain
                    WHERE api_chain.chain_code = api_assets.chain_code
                    AND api_chain.status = 1
                )"
            .to_string(),
        );

        conditions.push(
            " EXISTS (
                    SELECT 1
                    FROM api_coin
                    WHERE api_coin.chain_code = api_assets.chain_code
                    AND api_coin.token_address = api_assets.token_address
                    AND api_coin.symbol = api_assets.symbol
                    AND api_coin.status = 1
                )"
            .to_string(),
        );

        if let Some(chain_code) = chain_code {
            conditions.push(format!("chain_code = '{chain_code}'"));
        }

        if !addr.is_empty() {
            let str = format!("address in ('{}')", addr.join("','"));
            conditions.push(str)
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        // tracing::info!("sql: {}", sql);

        sqlx::query_as::<sqlx::Sqlite, ApiAssetsEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_balance<'a, E>(
        exec: E,
        address: &str,
        chain_code: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicUpdateBuilder::new("api_assets")
            .set("balance", balance)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", &address)
            .and_where_eq("chain_code", chain_code)
            .and_where_eq("token_address", token_address.unwrap_or_default());
        SqlExecutableNoReturn::execute(&builder, exec).await
    }

    /// 批量更新余额（在事务中执行）
    /// 使用 sqlx::query 直接执行，避免 Executor 所有权问题
    pub async fn batch_update_balance_in_tx<'a>(
        exec: &mut sqlx::Transaction<'a, Sqlite>,
        updates: &[(String, String, Option<String>, String)], // (address, chain_code, token_address, balance)
    ) -> Result<(), crate::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        // 在事务中批量执行更新，减少数据库往返次数
        for (address, chain_code, token_address, balance) in updates {
            let token_addr = token_address.clone().unwrap_or_default();
            let sql = r#"
                UPDATE api_assets 
                SET balance = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                WHERE address = ? AND chain_code = ? AND token_address = ?
            "#;

            sqlx::query(sql)
                .bind(balance)
                .bind(address)
                .bind(chain_code)
                .bind(token_addr)
                .execute(exec.as_mut())
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;
        }

        Ok(())
    }

    pub async fn upsert_assets<'a, E>(
        exec: E,
        assets: ApiCreateAssetsVo,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let ApiCreateAssetsVo { assets_id, name, decimals, protocol, status, is_multisig, balance } =
            assets;

        let token_address = assets_id.token_address.unwrap_or_default();
        let protocol = protocol.unwrap_or_default();

        let sql = r#"
        INSERT INTO api_assets
        (
            name, symbol, decimals, address, chain_code, token_address, protocol, status, balance, is_multisig, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT (address, chain_code, token_address)
        DO UPDATE SET
            status = EXCLUDED.status,
            is_multisig = EXCLUDED.is_multisig,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now');
    "#;

        sqlx::query(sql)
            .bind(name)
            .bind(assets_id.symbol)
            .bind(decimals)
            .bind(assets_id.address)
            .bind(assets_id.chain_code)
            .bind(token_address)
            .bind(protocol)
            .bind(status)
            .bind(balance)
            .bind(is_multisig)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|_| crate::Error::Database(DatabaseError::UpdateFailed))
    }

    pub async fn delete_assets<'a, E>(
        exec: E,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE api_assets 
            SET status = $4, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE address = $1 AND chain_code = $2 AND token_address = $3"#;

        sqlx::query(sql)
            .bind(address)
            .bind(chain_code)
            .bind(token_address)
            .bind(0) // Assuming 0 is the status for deletion
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|_| crate::Error::Database(DatabaseError::UpdateFailed))
    }

    // pub async fn delete_multi_assets<'a, E>(
    //     exec: E,
    //     assets_ids: Vec<AssetsId>,
    // ) -> Result<(), crate::Error>
    // where
    //     E: Executor<'a, Database = Sqlite>,
    // {
    //     if assets_ids.is_empty() {
    //         return Ok(());
    //     }
    //     let placeholders = assets_ids.iter().map(|_| "(?, ?, ?, ?)").collect::<Vec<_>>().join(", ");

    //     // 构建 SQL 查询
    //     let sql = format!(
    //         "UPDATE api_assets SET status = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE (address, symbol, chain_code, token_address) IN ({})",
    //         placeholders
    //     );

    //     let mut query = sqlx::query(&sql);

    //     // 绑定参数
    //     for assets_id in &assets_ids {
    //         let token_address = match &assets_id.token_address {
    //             Some(token_address) => token_address.to_string(),
    //             None => String::new(),
    //         };
    //         query = query
    //             .bind(&assets_id.address)
    //             .bind(&assets_id.symbol)
    //             .bind(&assets_id.chain_code)
    //             .bind(token_address);
    //     }

    //     // 执行查询
    //     query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    // }

    pub async fn update_status<'a, E>(
        exec: E,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE api_assets
        SET status = $4, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE chain_code = $1 AND LOWER(symbol) = LOWER($2) AND token_address = $3
            AND EXISTS (
                SELECT 1
                FROM api_chain
                WHERE api_chain.chain_code = api_assets.chain_code
                AND api_chain.status = 1
            )
            AND EXISTS (
                SELECT 1
                FROM api_coin
                WHERE api_coin.chain_code = api_assets.chain_code
                AND api_coin.token_address = api_assets.token_address
                AND api_coin.symbol = api_assets.symbol
                AND api_coin.status = 1
            );
        "#;

        sqlx::query(sql)
            .bind(chain_code)
            .bind(symbol)
            .bind(token_address.unwrap_or_default())
            .bind(status)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|_| crate::Error::Database(DatabaseError::UpdateFailed))?;

        Ok(())
    }

    pub async fn assets_by_id<'a, 'b, E>(
        exec: E,
        assets_id: &AssetsIdVo<'b>,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM 
                api_assets
            WHERE status = 1 AND address =$1 AND chain_code = $2 AND token_address = $3
                AND EXISTS (
                    SELECT 1
                    FROM api_chain
                    WHERE api_chain.chain_code = api_assets.chain_code
                    AND api_chain.status = 1
                )
                AND EXISTS (
                    SELECT 1
                    FROM api_coin
                    WHERE api_coin.chain_code = api_assets.chain_code
                    AND api_coin.token_address = api_assets.token_address
                    AND api_coin.symbol = api_assets.symbol
                    AND api_coin.status = 1
                );"#;

        let rs = sqlx::query_as::<sqlx::Sqlite, ApiAssetsEntity>(sql)
            .bind(assets_id.address)
            .bind(assets_id.chain_code)
            .bind(assets_id.token_address.clone().unwrap_or_default())
            .fetch_optional(exec)
            .await;

        match rs {
            Ok(rs) => Ok(rs),
            Err(_e) => Err(crate::Error::Database(DatabaseError::QueryFailed)),
        }
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol<'a, E>(
        exec: E,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let addresses = crate::any_in_collection(address, "','");
        let mut sql = "SELECT * FROM api_assets
        WHERE status = 1
            AND EXISTS (
                SELECT 1
                FROM api_chain
                WHERE api_chain.chain_code = api_assets.chain_code
                AND api_chain.status = 1
            )
            AND EXISTS (
                SELECT 1
                FROM api_coin
                WHERE api_coin.chain_code = api_assets.chain_code
                AND api_coin.token_address = api_assets.token_address
                AND api_coin.symbol = api_assets.symbol
                AND api_coin.status = 1
            )"
        .to_string();

        if !addresses.is_empty() {
            let str = format!(" AND address in ('{}')", addresses);
            sql.push_str(&str)
        }

        if chain_code.is_some() {
            sql.push_str(" AND chain_code = ?");
        }

        if symbol.is_some() {
            sql.push_str(" AND symbol = ?");
        }

        if let Some(is_multisig) = is_multisig {
            let is_multisig = if is_multisig { vec![1] } else { vec![0, 2] };
            let is_multisig = crate::any_in_collection(is_multisig, "','");
            let str = format!(" AND is_multisig in ('{}')", is_multisig);
            sql.push_str(&str);
        }

        let mut query = sqlx::query_as::<_, ApiAssetsEntity>(&sql);

        if let Some(code) = chain_code {
            query = query.bind(code);
        }

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }

        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub(crate) async fn get_api_assets_by_address<'a, E>(
        exec: E,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        token_address: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ApiAssetsEntityWithAddressType>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let addresses = crate::any_in_collection(address, "','");
        let base_sql = |table_name: &str| -> String {
            format!(
                "SELECT a.name, a.symbol, a.decimals, a.address, a.chain_code, 
                a.token_address, a.protocol, a.status, a.balance, a.is_multisig, 
                a.created_at, a.updated_at, acc.address_type
                FROM api_assets AS a
                JOIN {table_name} AS acc 
                ON a.address = acc.address AND a.chain_code = acc.chain_code
                WHERE a.status = 1 
                    AND EXISTS (
                        SELECT 1
                        FROM api_chain
                        WHERE api_chain.chain_code = a.chain_code
                        AND api_chain.status = 1
                    )
                    AND EXISTS (
                        SELECT 1
                        FROM api_coin
                        WHERE api_coin.chain_code = a.chain_code
                        AND api_coin.token_address = a.token_address
                        AND api_coin.symbol = a.symbol
                        AND api_coin.status = 1
                    )"
            )
        };

        let add_dynamic_conditions = |sql: &mut String| {
            if !addresses.is_empty() {
                sql.push_str(&format!(" AND a.address IN ('{}')", addresses));
            }
            if chain_code.is_some() {
                sql.push_str(" AND a.chain_code = ?");
            }
            if symbol.is_some() {
                sql.push_str(" AND a.symbol = ?");
            }
            if token_address.is_some() {
                sql.push_str(" AND a.token_address = ?");
            }
            if let Some(is_multisig) = is_multisig {
                let is_multisig_values = if is_multisig { vec![1] } else { vec![0, 2] };
                let is_multisig_str = crate::any_in_collection(is_multisig_values, "','");
                sql.push_str(&format!(" AND a.is_multisig IN ('{}')", is_multisig_str));
            }
        };

        let sql = match is_multisig {
            Some(true) => {
                let mut sql = base_sql("multisig_account");
                add_dynamic_conditions(&mut sql);
                format!("{sql} AND acc.is_del = 0")
            }
            Some(false) => {
                let mut sql = base_sql("api_account");
                add_dynamic_conditions(&mut sql);
                sql
            }
            None => {
                let mut sql1 = base_sql("api_account");

                let mut sql2 = base_sql("multisig_account");
                add_dynamic_conditions(&mut sql1);
                add_dynamic_conditions(&mut sql2);
                format!("{sql1} UNION {sql2}")
            }
        };

        let mut query = sqlx::query_as::<_, ApiAssetsEntityWithAddressType>(&sql);

        if let Some(code) = chain_code {
            query = query.bind(code);
        }

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }

        if let Some(token_address) = token_address {
            query = query.bind(token_address);
        }

        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn assets_with_wallet_address_by_address<'a, 'b, E>(
        exec: E,
        keys: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicQueryBuilder::new("SELECT a.address, aa.wallet_address, a.symbol, a.chain_code, a.token_address, a.balance, a.decimals \
                FROM api_assets a LEFT JOIN api_account aa ON a.address = aa.address")
            .and_where_in("(a.address || ':' || a.chain_code || ':' || a.token_address)", keys);

        builder.fetch_all(exec).await
    }

    pub async fn assets_with_wallet_address_by_token<'a, 'b, E>(
        exec: E,
        keys: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicQueryBuilder::new("SELECT a.address, aa.wallet_address, a.symbol, a.chain_code, a.token_address, a.balance, a.decimals \
                FROM api_assets a LEFT JOIN api_account aa ON a.address = aa.address")
            .and_where_in("(a.symbol || ':' || a.chain_code || ':' || a.token_address)", keys);

        builder.fetch_all(exec).await
    }

    pub async fn get_api_wallet_total_assets_v2<'a, E>(
        exec: E,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<SumResult, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let wallet_address_sql = if let Some(wallet_address) = wallet_address {
            format!("AND api_account.wallet_address = '{wallet_address}'")
        } else {
            "".to_string()
        };

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
CAST(SUM(total_account_amount) AS REAL) as total_amount,
CAST(SUM(total_coins_quantity) AS REAL) as total_coins_quantity
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
 {wallet_address_sql}
 {account_id_sql}
 {chain_code_sql}
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id
)AS all_data2
        "#
        );

        sqlx::query_as::<_, SumResult>(sql.as_str())
            .bind(wallet_address)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_api_wallet_assets_v2<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
        hide_zero_balance: bool,
    ) -> Result<Vec<ApiAssertSummeryEntity>, crate::Error>
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
all_data.chain_code                                                 AS chain_code,
all_data.symbol                                                     AS symbol,
all_data.api_assets_name 							                AS api_assets_name,
CAST(SUM(all_data.total_coin_quantity) AS REAL) 		            AS total_coins_quantity,
CAST(all_data.coin_unit_price AS REAL)                              AS coin_unit_price,
CAST(SUM(total_coin_amount) AS REAL)			                    AS total_account_amount,
all_data.is_default 				                                AS coin_is_default,
all_data.is_multisig 				                                AS assets_is_multisig,
JSON_GROUP_OBJECT(all_data.chain_code, all_data.token_address)      AS chain_token_map
FROM
(
SELECT
api_account.account_id,api_account.name,api_account.api_wallet_type,api_account.wallet_address,api_account.address, api_account.chain_code,
api_assets.token_address,api_assets.balance,api_assets.symbol,api_assets.name as api_assets_name,api_assets.is_multisig,
api_chain.name 														AS api_chain_name,
api_coin.price 														AS coin_unit_price,
api_coin.is_default,
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
ORDER BY total_coin_quantity DESC
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id,all_data.symbol
ORDER BY account_id ASC
        "#
        );

        sqlx::query_as::<_, ApiAssertSummeryEntity>(sql.as_str())
            .bind(wallet_address)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ApiAssertSummeryEntity {
    pub chain_code: String,
    pub symbol: String,
    pub api_assets_name: String,
    pub total_coins_quantity: f64,
    pub coin_unit_price: Option<f64>,
    pub total_account_amount: Option<f64>,
    pub coin_is_default: bool,
    pub assets_is_multisig: i8,
    /// 示例：{"eth": "0x...", "bsc": "0x..."}
    pub chain_token_map: serde_json::Value,
}
impl ApiAssertSummeryEntity {
    pub fn get_chain_token_map(&self) -> Result<HashMap<String, String>, crate::Error> {
        if let serde_json::Value::Object(map) = &self.chain_token_map {
            let mut result: HashMap<String, String> = HashMap::new();
            for (key, val) in map {
                if let serde_json::Value::String(s) = val {
                    result.insert(key.to_string(), s.to_string());
                } else {
                    // 如果不是字符串，可以转为字符串或跳过
                    result.insert(key.to_string(), val.to_string());
                }
            }
            Ok(result)
        } else {
            tracing::warn!("chain_token_map json map is not object: {:?}", self.chain_token_map);
            Ok(HashMap::new())
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct SumResult {
    pub total_coins_quantity: f64,
    pub total_amount: f64,
}
