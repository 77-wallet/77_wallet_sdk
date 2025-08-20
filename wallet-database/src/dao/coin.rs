use crate::{
    DbPool,
    entities::coin::{CoinData, CoinEntity, CoinId, CoinWithAssets, SymbolId},
    pagination::Pagination,
};
use chrono::SecondsFormat;
use sqlx::{
    Executor, Pool, Sqlite,
    types::chrono::{DateTime, Utc},
};
use std::{collections::HashSet, sync::Arc};

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct CoinDao {
    pub name: String,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: Option<String>,
    pub status: i8,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl CoinEntity {
    pub fn token_address(&self) -> Option<String> {
        self.token_address
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned()
    }

    pub async fn update_price_unit<'a, E>(
        exec: E,
        coin_id: &CoinId,
        price: &str,
        unit: Option<u8>,
        status: Option<i32>,
        time: Option<DateTime<Utc>>,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // 基础 SQL 语句，设置 price
        let mut sql = "UPDATE coin SET price = ?".to_string();

        if unit.is_some() {
            sql.push_str(", decimals = ?");
        }

        if status.is_some() {
            sql.push_str(", status = ?");
        }

        if time.is_some() {
            sql.push_str(", updated_at = ?");
        }

        sql.push_str(
            " WHERE token_address = ? AND LOWER(symbol) = LOWER(?) AND chain_code = ? RETURNING *",
        );

        let mut query = sqlx::query_as::<sqlx::Sqlite, Self>(&sql).bind(price); // 绑定 price 参数

        if let Some(unit_val) = unit {
            query = query.bind(unit_val);
        }
        if let Some(status_val) = status {
            query = query.bind(status_val);
        }
        if let Some(time_val) = time {
            query = query.bind(time_val.to_rfc3339_opts(SecondsFormat::Secs, true));
        }

        // 处理 token_address，如果为空，设置为空字符串
        let token_address = coin_id.token_address.clone().unwrap_or_default();

        // 绑定 WHERE 子句的参数
        query = query
            .bind(token_address)
            .bind(&coin_id.symbol)
            .bind(&coin_id.chain_code);

        // 执行查询
        query
            .fetch_all(exec)
            // .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn detail<'a, E>(
        executor: E,
        symbol: &str,
        chain_code: &str,
        token_address: Option<String>,
    ) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM coin where symbol = $1
        AND chain_code = $2 AND token_address = $3 AND is_del = 0 AND status = 1;";
        let token_address = token_address.unwrap_or_default();
        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(symbol)
            .bind(chain_code)
            .bind(token_address)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // pub async fn symbol_list<'a, E>(
    //     exec: E,
    //     chain_code: Option<String>,
    // ) -> Result<Vec<Coins>, crate::Error>
    // where
    //     E: Executor<'a, Database = Sqlite>,
    // {
    //     let mut sql = "SELECT DISTINCT symbol
    //     FROM coin WHERE is_del = 0 AND status = 1 "
    //         .to_string();

    //     let mut conditions = Vec::new();

    //     if let Some(chain_code) = chain_code {
    //         conditions.push(format!("chain_code = '{chain_code}'"));
    //     }

    //     if !conditions.is_empty() {
    //         sql.push_str(" AND ");
    //         sql.push_str(&conditions.join(" AND "));
    //     }

    //     sqlx::query_as::<sqlx::Sqlite, Coins>(&sql)
    //         .fetch_all(exec)
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))
    // }

    pub async fn chain_code_list<'a, E>(exec: E) -> Result<Vec<String>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT DISTINCT chain_code FROM coin WHERE is_del = 0 AND status = 1 ";

        sqlx::query_scalar::<_, String>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list_v2<'a, E>(
        exec: E,
        symbol: Option<String>,
        chain_code: Option<String>,
        is_default: Option<u8>,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1".to_string();

        let mut conditions = Vec::new();

        if let Some(symbol) = symbol {
            conditions.push(format!("symbol = '{symbol}'"));
        }

        if let Some(chain_code) = chain_code {
            conditions.push(format!("chain_code = '{chain_code}'"));
        }

        if let Some(is_default) = is_default {
            conditions.push(format!("is_default = {is_default}"));
        }

        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        sqlx::query_as::<sqlx::Sqlite, Self>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list<'a, E>(
        exec: E,
        symbol_list: &[String],
        chain_code: Option<String>,
        is_default: Option<u8>,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let symbol_list = crate::any_in_collection(symbol_list, "','");
        let mut sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1".to_string();

        let mut conditions = Vec::new();

        if !symbol_list.is_empty() {
            conditions.push(format!("symbol in ('{}')", symbol_list));
        }

        if let Some(chain_code) = chain_code {
            conditions.push(format!("chain_code = '{chain_code}'"));
        }

        if let Some(is_default) = is_default {
            conditions.push(format!("is_default = {is_default}"));
        }

        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        sqlx::query_as::<sqlx::Sqlite, Self>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn coin_list_symbol_not_in<'a, E>(
        exec: &E,
        chain_codes: &HashSet<String>,
        keyword: Option<&str>,
        symbol_list: &HashSet<String>,
        is_default: Option<u8>,
        is_popular: Option<u8>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<Self>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let symbol_list = crate::any_in_collection(symbol_list, "','");
        let chain_codes = crate::any_in_collection(chain_codes, "','");
        let mut sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1".to_string();

        let mut conditions = Vec::new();

        if let Some(is_default) = is_default {
            conditions.push(format!("is_default = '{is_default}'"));
        }

        if let Some(is_popular) = is_popular {
            conditions.push(format!("is_popular = '{is_popular}'"));
        }

        if !chain_codes.is_empty() {
            let str = format!(" AND chain_code in ('{}')", chain_codes);
            sql.push_str(&str)
        }

        if !symbol_list.is_empty() {
            let str = format!(" AND symbol not in ('{}')", symbol_list);
            sql.push_str(&str)
        }

        if let Some(keyword) = keyword {
            conditions.push(format!("symbol LIKE '%{keyword}%'"));
        }

        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        let paginate = Pagination::<Self>::init(page, page_size);
        Ok(paginate.page(exec, &sql).await?)
    }

    pub async fn coin_list_page(
        pool: Arc<Pool<Sqlite>>,
        chain_code: Option<&str>,
        keyword: Option<&str>,
        symbol_list: Vec<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<Self>, crate::Error> {
        let symbol_list = crate::any_in_collection(symbol_list, "','");
        let mut sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1 ".to_string();

        let mut conditions = Vec::new();

        if let Some(chain_code) = chain_code {
            conditions.push(format!("chain_code = '{chain_code}'"));
        }

        if !symbol_list.is_empty() {
            let str = format!(" AND symbol in ('{}')", symbol_list);
            sql.push_str(&str)
        }

        if let Some(keyword) = keyword {
            conditions.push(format!("symbol LIKE '%{keyword}%'"));
        }

        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        // 执行查询并返回结果
        let paginate = Pagination::<CoinEntity>::init(page, page_size);
        Ok(paginate.page(&*pool, &sql).await?)
    }

    pub async fn upsert_multi_coin<'a, E>(tx: E, coins: Vec<CoinData>) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if coins.is_empty() {
            return Ok(());
        }
        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "insert into coin (
                name, symbol, chain_code, token_address, price, protocol, decimals, is_default, is_popular, is_custom, status, created_at, updated_at) ",
        );
        query_builder.push_values(coins, |mut b, coin| {
            b.push_bind(coin.name)
                .push_bind(coin.symbol)
                .push_bind(coin.chain_code)
                .push_bind(coin.token_address.unwrap_or_default())
                .push_bind(coin.price)
                .push_bind(coin.protocol)
                .push_bind(coin.decimals)
                .push_bind(coin.is_default)
                .push_bind(coin.is_popular)
                .push_bind(coin.is_custom)
                .push_bind(coin.status)
                .push_bind(coin.created_at.to_rfc3339_opts(SecondsFormat::Secs, true))
                .push_bind(coin.created_at.to_rfc3339_opts(SecondsFormat::Secs, true));
        });
        // query_builder.push(" on conflict (id) do update set updated_at = excluded.updated_at");
        query_builder.push(
            " on conflict (symbol, chain_code, token_address) do update set name = EXCLUDED.name, 
            decimals = EXCLUDED.decimals,
            is_default = EXCLUDED.is_default,
            is_popular = EXCLUDED.is_popular,
            is_custom = EXCLUDED.is_custom,
            status = EXCLUDED.status, 
            updated_at = EXCLUDED.updated_at, 
            is_del = EXCLUDED.is_del",
        );

        let query = query_builder.build();
        query
            .execute(tx)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn main_coin<'a, E>(
        chain_code: &str,
        exec: E,
    ) -> Result<Option<CoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1  and token_address = '' and chain_code = $1";

        let res = sqlx::query_as::<_, CoinEntity>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn get_coin<'a, E>(
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        exec: E,
    ) -> Result<Option<CoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM coin WHERE is_del = 0 AND status = 1  and chain_code = $1 and lower(symbol) = lower($2) and token_address = $3";

        let token_address = token_address.unwrap_or_default();

        let res = sqlx::query_as::<_, CoinEntity>(sql)
            .bind(chain_code)
            .bind(symbol)
            .bind(token_address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn get_coin_by_chain_code_token_address<'a, E>(
        exec: E,
        chain_code: &str,
        token_address: &str,
    ) -> Result<Option<CoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM coin WHERE is_del = 0 AND chain_code = $1 and token_address = $2 AND status =1 ";

        let res = sqlx::query_as::<_, CoinEntity>(sql)
            .bind(chain_code)
            .bind(token_address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn drop_coin_just_null_token_address<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM coin WHERE token_address IS NULL";

        sqlx::query(sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn drop_custom_coin<'a, E>(exec: E, coin_id: &SymbolId) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM coin WHERE chain_code = $1 and symbol = $2 AND is_custom = 1";

        sqlx::query(sql)
            .bind(&coin_id.chain_code)
            .bind(&coin_id.symbol)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // 查询coin表中is_custom为1，并且assets表中chain_code、symbol和coin表中chain_code、symbol相同的记录
    pub async fn get_custom_coin_and_assets<'a, E>(exec: E) -> Result<Vec<CoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM coin WHERE is_custom = 1 AND EXISTS 
        (SELECT 1 FROM assets WHERE assets.chain_code = coin.chain_code AND assets.symbol = coin.symbol)";
        sqlx::query_as::<_, CoinEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn drop_multi_custom_coin<'a, E>(
        tx: E,
        coin_ids: HashSet<SymbolId>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if coin_ids.is_empty() {
            return Ok(());
        }

        // 创建动态 SQL 语句
        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "DELETE FROM coin WHERE is_custom = 1 AND (chain_code, symbol) IN ",
        );

        // 使用 push_tuples 方法添加所有 coin_id 参数
        query_builder.push_tuples(coin_ids, |mut b, coin_id| {
            b.push_bind(coin_id.chain_code).push_bind(coin_id.symbol);
        });

        // 构建并执行查询
        let query = query_builder.build();
        query
            .execute(tx)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn clean_table<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM coin";
        sqlx::query(sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_last_coin<'a, E>(
        exec: E,
        is_create: bool,
    ) -> Result<Option<CoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let time = if is_create {
            "created_at"
        } else {
            "updated_at"
        };

        let sql = format!(
            "select * from coin where is_custom = 0 order by {} desc limit 1",
            time
        );

        let res = sqlx::query_as::<_, CoinEntity>(&sql)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn coin_count<'a, E>(exec: E) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM coin";
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn coin_list_with_assets(
        search: &str,
        exclude_token: Vec<String>,
        chain_code: String,
        address: Vec<String>,
        page: i64,
        page_size: i64,
        pool: DbPool,
    ) -> Result<Pagination<CoinWithAssets>, crate::Error> {
        let address = crate::any_in_collection(address, "','");

        let mut sql = format!(
            r#"
        SELECT coin.*, COALESCE(assets.balance, '0') AS balance
        FROM coin 
        LEFT JOIN assets 
            ON coin.chain_code = assets.chain_code 
            AND coin.token_address = assets.token_address 
            and assets.address  IN ('{}')
        WHERE coin.status = 1
        "#,
            address,
        );

        if !chain_code.is_empty() {
            sql.push_str(&format!(" AND coin.chain_code = '{}'", chain_code,));
        }

        if !exclude_token.is_empty() {
            let excludes: Vec<String> = exclude_token
                .into_iter()
                .map(|t| format!("'{}'", t.replace("'", "''")))
                .collect();
            let clause = excludes.join(", ");
            sql.push_str(&format!(" AND coin.token_address NOT IN ({})", clause));
        }

        if !search.is_empty() {
            let escaped = search.replace("'", "''").to_lowercase();
            sql.push_str(&format!(
                " AND (LOWER(coin.symbol) LIKE '%{}%' OR LOWER(coin.token_address) LIKE '%{}%')",
                escaped, escaped
            ));
        }

        sql.push_str(" ORDER BY CAST(assets.balance AS NUMERIC) DESC");

        // 分页
        let paginate = Pagination::<CoinWithAssets>::init(page, page_size);
        Ok(paginate.page(&pool, &sql).await?)
    }
}

// pub async fn symbol_list<'a, E>(
//     exec: E,
//     chain_code: Option<String>,
// ) -> Result<Vec<Coins>, crate::Error>
// where
//     E: Executor<'a, Database = Sqlite>,
// {
//     let mut sql =
//         "SELECT DISTINCT symbol FROM coin WHERE is_del = 0 AND status = 1 ".to_string();

//     let mut conditions = Vec::new();

//     if let Some(chain_code) = chain_code {
//         conditions.push(format!("chain_code = '{chain_code}'"));
//     }

//     if !conditions.is_empty() {
//         sql.push_str(" AND ");
//         sql.push_str(&conditions.join(" AND "));
//     }

//     sqlx::query_as::<sqlx::Sqlite, Coins>(&sql)
//         .fetch_all(exec)
//         .await
//         .map_err(|e| crate::Error::Database(e.into()))
// }
