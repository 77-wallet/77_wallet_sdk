use crate::{
    entities::{
        api_coin::{ApiCoinData, ApiCoinEntity},
        coin::{BatchCoinSwappable, CoinId, CoinWithAssets},
    }, pagination::Pagination, DbPool
};
use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::{Executor, QueryBuilder, Sqlite};

pub(crate) struct ApiCoinDao;

impl ApiCoinDao {
    pub async fn upsert_multi_coin<'a, E>(
        tx: E,
        coins: Vec<ApiCoinData>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if coins.is_empty() {
            return Ok(());
        }
        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "insert into api_coin (
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
            " on conflict (chain_code, token_address) do update set name = EXCLUDED.name, 
            decimals = EXCLUDED.decimals,
            is_custom = EXCLUDED.is_custom,
            is_default = EXCLUDED.is_default,
            status = EXCLUDED.status, 
            updated_at = EXCLUDED.updated_at, 
            is_del = EXCLUDED.is_del",
        );

        let query = query_builder.build();
        query.execute(tx).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list<'a, E>(
        exec: E,
        symbol: Option<String>,
        chain_code: Option<String>,
        is_default: Option<u8>,
    ) -> Result<Vec<ApiCoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "SELECT * FROM api_coin WHERE is_del = 0".to_string();

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

        sqlx::query_as::<sqlx::Sqlite, ApiCoinEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_coin<'a, E>(
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        exec: E,
    ) -> Result<Option<ApiCoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM api_coin WHERE is_del = 0 AND chain_code = $1 and lower(symbol) = lower($2) and token_address = $3";

        let token_address = token_address.unwrap_or_default();

        tracing::info!(sql=%sql, chain_code=%chain_code, symbol=%symbol, token_address=%token_address, "get_coin");

        let res = sqlx::query_as::<_, ApiCoinEntity>(sql)
            .bind(chain_code)
            .bind(symbol)
            .bind(token_address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn main_coin<'a, E>(
        chain_code: &str,
        exec: E,
    ) -> Result<Option<ApiCoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql =
            "SELECT * FROM api_coin WHERE is_del = 0 AND token_address = '' and chain_code = $1";

        let res = sqlx::query_as::<_, ApiCoinEntity>(sql)
            .bind(chain_code)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn update_price_unit<'a, E>(
        exec: E,
        coin_id: &CoinId,
        price: &str,
        unit: Option<u8>,
        status: Option<i32>,
        time: Option<DateTime<Utc>>,
        symbol: Option<String>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // 基础 SQL 语句，设置 price
        let mut sql = "UPDATE api_coin SET price = ?".to_string();

        if unit.is_some() {
            sql.push_str(", decimals = ?");
        }

        if status.is_some() {
            sql.push_str(", status = ?");
        }

        if time.is_some() {
            sql.push_str(", updated_at = ?");
        }

        if symbol.is_some() {
            sql.push_str(", symbol = ?");
        }

        sql.push_str(" WHERE token_address = ?  AND chain_code = ?");

        // let mut query = sqlx::query_as::<sqlx::Sqlite, Self>(&sql).bind(price); // 绑定 price 参数
        let mut query = sqlx::query(&sql).bind(price); // 绑定 price 参数

        if let Some(unit_val) = unit {
            query = query.bind(unit_val);
        }
        if let Some(status_val) = status {
            query = query.bind(status_val);
        }
        if let Some(time_val) = time {
            query = query.bind(time_val.to_rfc3339_opts(SecondsFormat::Secs, true));
        }
        if let Some(symbol_val) = symbol {
            query = query.bind(symbol_val);
        }

        // 处理 token_address，如果为空，设置为空字符串
        let token_address = coin_id.token_address.clone().unwrap_or_default();
        // 绑定 WHERE 子句的参数
        query = query
            .bind(token_address)
            // .bind(&coin_id.symbol)
            .bind(&coin_id.chain_code);

        let _res = query.execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }


    pub async fn update_price_unit1<'a, E>(
        exec: E,
        chain_code: &str,
        token_address: &str,
        price: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql =
            "UPDATE api_coin SET price = ? where token_address = ? and chain_code = ?".to_string();

        let _r = sqlx::query::<_>(&sql)
            .bind(price)
            .bind(token_address)
            .bind(chain_code)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn multi_update_swappable<'a, E>(
        coins: Vec<BatchCoinSwappable>,
        tx: E,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if coins.is_empty() {
            return Ok(());
        }

        let mut qb = QueryBuilder::<Sqlite>::new("UPDATE api_coin SET swappable = CASE ");

        for p in coins.iter() {
            qb.push("WHEN chain_code=")
                .push_bind(p.chain_code.clone())
                .push(" AND token_address=")
                .push_bind(p.token_address.clone())
                .push(" THEN ")
                .push_bind(1)
                .push(" ");
        }
        qb.push("ELSE 0 END");

        qb.build().execute(tx).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_last_coin<'a, E>(
        exec: E,
        is_create: bool,
    ) -> Result<Option<ApiCoinEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let time = if is_create { "created_at" } else { "updated_at" };

        let sql =
            format!("select * from api_coin where is_custom = 0 order by {} desc limit 1", time);

        let res = sqlx::query_as::<_, ApiCoinEntity>(&sql)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn coin_count<'a, E>(exec: E) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM api_coin";
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn same_coin_num<'a, E>(
        exec: E,
        symbol: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM api_coin where symbol = ? and chain_code = ?";
        sqlx::query_scalar::<_, i64>(sql)
            .bind(symbol)
            .bind(chain_code)
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
        FROM api_coin
        LEFT JOIN assets
            ON coin.chain_code = assets.chain_code
            AND coin.token_address = assets.token_address
            and assets.address  IN ('{}')
        WHERE  swappable = 1
        "#,
            address,
        );

        if !chain_code.is_empty() {
            sql.push_str(&format!(" AND coin.chain_code = '{}'", chain_code));
        } else {
            // TODO: 优化目前只查询这些链的数据,后续支持了更多的链进行删除
            sql.push_str(" AND coin.chain_code in ('tron','bnb','eth')");
        }

        if !exclude_token.is_empty() {
            let excludes: Vec<String> =
                exclude_token.into_iter().map(|t| format!("'{}'", t.replace("'", "''"))).collect();
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

    pub async fn drop_coin_just_null_token_address<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM api_coin WHERE token_address IS NULL";

        sqlx::query(sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
