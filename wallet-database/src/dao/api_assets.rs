use crate::{
    entities::{
        api_assets::{ApiAssetsEntity, ApiAssetsEntityWithAddressType, AssetWithWalletAddress},
        assets::AssetsIdVo,
    },
    error::DatabaseError,
    sql_utils::{SqlExecutableNoReturn, update_builder::DynamicUpdateBuilder},
};
use std::collections::HashMap;

use crate::entities::api_assets::ApiCreateAssetsVo;
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
        SqlExecutableNoReturn::execute(builder, exec).await
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

        let token_address = assets_id.token_address.as_db_str().to_string();
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
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
    }

    /// 批量插入或更新资产
    pub async fn upsert_assets_multi(
        exec: &mut sqlx::SqliteConnection,
        assets: Vec<ApiCreateAssetsVo>,
    ) -> Result<(), crate::Error> {
        if assets.is_empty() {
            return Ok(());
        }

        // Keep each SQL statement smaller to reduce sqlite write-lock hold time.
        const BATCH_SIZE: usize = 200;
        tracing::info!(count = %assets.len(), "ApiAssetsDao: starting upsert_assets_multi");

        for (batch_idx, chunk) in assets.chunks(BATCH_SIZE).enumerate() {
            tracing::debug!(batch_idx = %batch_idx, batch_size = %chunk.len(), "ApiAssetsDao: processing batch");

            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "INSERT INTO api_assets (
                    name, symbol, decimals, address, chain_code, token_address, protocol, status, balance, is_multisig, created_at, updated_at
                ) ",
            );

            qb.push_values(chunk, |mut b, item| {
                let token_address = item.assets_id.token_address.as_db_str().to_string();
                let protocol = item.protocol.clone().unwrap_or_default();

                b.push_bind(item.name.clone())
                    .push_bind(item.assets_id.symbol.clone())
                    .push_bind(item.decimals)
                    .push_bind(item.assets_id.address.clone())
                    .push_bind(item.assets_id.chain_code.clone())
                    .push_bind(token_address)
                    .push_bind(protocol)
                    .push_bind(item.status)
                    .push_bind(item.balance.clone())
                    .push_bind(item.is_multisig)
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
            });

            qb.push(
                " ON CONFLICT(address, chain_code, token_address)
                  DO UPDATE SET
                      status = excluded.status,
                      is_multisig = excluded.is_multisig,
                      updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
            );

            let result = qb
                .build()
                .execute(&mut *exec)
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;

            tracing::debug!(batch_idx = %batch_idx, rows_affected = %result.rows_affected(), "ApiAssetsDao: batch completed");
        }

        tracing::info!(count = %assets.len(), "ApiAssetsDao: upsert_assets_multi completed");
        Ok(())
    }

    /// 批量插入或更新资产（用于“余额同步”场景）
    ///
    /// 与 `upsert_assets_multi` 的关键区别：
    /// - ON CONFLICT 时仅更新 `balance` + `updated_at`
    /// - 这样不会被默认资产初始化（balance=0）覆盖链上同步到的真实余额
    pub async fn upsert_assets_multi_update_balance(
        exec: &mut sqlx::SqliteConnection,
        assets: Vec<ApiCreateAssetsVo>,
    ) -> Result<(), crate::Error> {
        if assets.is_empty() {
            return Ok(());
        }

        // Keep each SQL statement smaller to reduce sqlite write-lock hold time.
        const BATCH_SIZE: usize = 200;
        tracing::info!(count = %assets.len(), "ApiAssetsDao: starting upsert_assets_multi_update_balance");

        for (batch_idx, chunk) in assets.chunks(BATCH_SIZE).enumerate() {
            tracing::debug!(batch_idx = %batch_idx, batch_size = %chunk.len(), "ApiAssetsDao: processing balance-upsert batch");

            let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "INSERT INTO api_assets (
                    name, symbol, decimals, address, chain_code, token_address, protocol, status, balance, is_multisig, created_at, updated_at
                ) ",
            );

            qb.push_values(chunk, |mut b, item| {
                let token_address = item.assets_id.token_address.as_db_str().to_string();
                let protocol = item.protocol.clone().unwrap_or_default();

                b.push_bind(item.name.clone())
                    .push_bind(item.assets_id.symbol.clone())
                    .push_bind(item.decimals)
                    .push_bind(item.assets_id.address.clone())
                    .push_bind(item.assets_id.chain_code.clone())
                    .push_bind(token_address)
                    .push_bind(protocol)
                    .push_bind(item.status)
                    .push_bind(item.balance.clone())
                    .push_bind(item.is_multisig)
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
            });

            qb.push(
                " ON CONFLICT(address, chain_code, token_address)
                  DO UPDATE SET
                      balance = excluded.balance,
                      updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
            );

            let result = qb
                .build()
                .execute(&mut *exec)
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;

            tracing::debug!(batch_idx = %batch_idx, rows_affected = %result.rows_affected(), "ApiAssetsDao: balance-upsert batch completed");
        }

        tracing::info!(count = %assets.len(), "ApiAssetsDao: upsert_assets_multi_update_balance completed");
        Ok(())
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
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
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
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))?;

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
            .bind(assets_id.token_address.as_db_str())
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
        // `api_assets` lives in `api_wallet.db`. Multisig tables (e.g. `multisig_account`) are in
        // the core db (`data.db`) and must NOT be joined here. This API wallet path has no multisig
        // business, so we always join `api_account`.
        let base_sql = || -> String {
            "SELECT a.name, a.symbol, a.decimals, a.address, a.chain_code,
                a.token_address, a.protocol, a.status, a.balance, a.is_multisig,
                a.created_at, a.updated_at, acc.address_type
                FROM api_assets AS a
                JOIN api_account AS acc
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
            .to_string()
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

        let mut sql = base_sql();
        add_dynamic_conditions(&mut sql);

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
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        // 兼容两种调用模式：
        // 1) address（全量刷新）
        // 2) address:chain:token（脏资产增量刷新）
        let has_composite_key = keys.iter().any(|k| k.contains(':'));
        let mut composite_keys: Vec<(String, String, String)> = Vec::new();
        let mut addresses: Vec<String> = Vec::new();
        for key in keys {
            if has_composite_key {
                let mut parts = key.splitn(3, ':');
                let address = parts.next().unwrap_or_default();
                let chain_code = parts.next().unwrap_or_default();
                let token_address = parts.next().unwrap_or_default();
                if !address.is_empty() && !chain_code.is_empty() {
                    composite_keys.push((
                        address.to_string(),
                        chain_code.to_string(),
                        token_address.to_string(),
                    ));
                } else {
                    tracing::warn!("invalid address-chain-token key: {}", key);
                }
            } else if !key.is_empty() {
                addresses.push(key.clone());
            }
        }

        if has_composite_key {
            if composite_keys.is_empty() {
                return Ok(Vec::new());
            }
            // 用 VALUES/CTE + JOIN 替代字符串拼接 IN，便于 SQLite 使用索引并减少表达式计算。
            let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
                "WITH keyset(address, chain_code, token_address) AS (VALUES ",
            );
            let mut separated = qb.separated(", ");
            for (address, chain_code, token_address) in &composite_keys {
                separated
                    .push("(")
                    .push_bind(address)
                    .push(", ")
                    .push_bind(chain_code)
                    .push(", ")
                    .push_bind(token_address)
                    .push_unseparated(")");
            }
            qb.push(
                ") \
                 SELECT DISTINCT \
                    a.address, \
                    aa.wallet_address, \
                    a.symbol, \
                    a.chain_code, \
                    a.token_address, \
                    a.balance, \
                    a.decimals \
                 FROM keyset k \
                 JOIN api_assets a \
                    ON a.address = k.address \
                    AND a.chain_code = k.chain_code \
                    AND a.token_address = k.token_address \
                    AND a.status = 1 \
                 JOIN api_account aa \
                    ON aa.address = a.address \
                    AND aa.chain_code = a.chain_code \
                    AND aa.status = 1",
            );

            return qb
                .build_query_as::<AssetWithWalletAddress>()
                .fetch_all(exec)
                .await
                .map_err(|e| crate::Error::Database(e.into()));
        }

        if addresses.is_empty() {
            return Ok(Vec::new());
        }

        let mut qb = sqlx::QueryBuilder::<Sqlite>::new("WITH addrset(address) AS (VALUES ");
        let mut separated = qb.separated(", ");
        for address in &addresses {
            separated.push("(").push_bind(address).push_unseparated(")");
        }
        // 地址全量刷新路径同样走 CTE + JOIN，并显式过滤 status，避免无效行参与聚合计算。
        qb.push(
            ") \
             SELECT DISTINCT \
                a.address, \
                aa.wallet_address, \
                a.symbol, \
                a.chain_code, \
                a.token_address, \
                a.balance, \
                a.decimals \
             FROM addrset s \
             JOIN api_assets a \
                ON a.address = s.address \
                AND a.status = 1 \
             JOIN api_account aa \
                ON aa.address = a.address \
                AND aa.chain_code = a.chain_code \
                AND aa.status = 1",
        );

        qb.build_query_as::<AssetWithWalletAddress>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn assets_with_wallet_address_by_token<'a, 'b, E>(
        exec: E,
        keys: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut token_keys = Vec::<(String, String, String)>::new();
        for key in keys {
            let mut parts = key.splitn(3, ':');
            let symbol = parts.next().unwrap_or_default();
            let chain_code = parts.next().unwrap_or_default();
            let token_address = parts.next().unwrap_or_default();
            if symbol.is_empty() || chain_code.is_empty() {
                tracing::warn!("invalid token key: {}", key);
                continue;
            }
            token_keys.push((
                symbol.to_ascii_uppercase(),
                chain_code.to_string(),
                token_address.to_string(),
            ));
        }

        if token_keys.is_empty() {
            return Ok(Vec::new());
        }

        // token 脏数据刷新路径：按 (symbol, chain_code, token_address) 构造 keyset JOIN，
        // 避免 (symbol||':'||chain||':'||token) IN (...) 导致索引难命中。
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
            "WITH keyset(symbol, chain_code, token_address) AS (VALUES ",
        );
        let mut separated = qb.separated(", ");
        for (symbol, chain_code, token_address) in &token_keys {
            separated
                .push("(")
                .push_bind(symbol)
                .push(", ")
                .push_bind(chain_code)
                .push(", ")
                .push_bind(token_address)
                .push_unseparated(")");
        }

        qb.push(
            ") \
             SELECT DISTINCT \
                a.address, \
                aa.wallet_address, \
                a.symbol, \
                a.chain_code, \
                a.token_address, \
                a.balance, \
                a.decimals \
             FROM keyset k \
             JOIN api_assets a \
                ON a.symbol = k.symbol \
                AND a.chain_code = k.chain_code \
                AND a.token_address = k.token_address \
                AND a.status = 1 \
             JOIN api_account aa \
                ON aa.address = a.address \
                AND aa.chain_code = a.chain_code \
                AND aa.status = 1",
        );

        qb.build_query_as::<AssetWithWalletAddress>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // TODO: 慢sql，需要优化
    pub async fn get_api_wallet_total_assets_v2<'a, E>(
        exec: E,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<SumResult, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let start = std::time::Instant::now();
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
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
"#,
        );

        if let Some(wallet_address) = wallet_address {
            qb.push(" AND api_account.wallet_address = ").push_bind(wallet_address);
        }
        if let Some(account_id) = account_id {
            qb.push(" AND api_account.account_id = ").push_bind(account_id);
        }
        if let Some(chain_code) = chain_code {
            qb.push(" AND api_account.chain_code = ").push_bind(chain_code);
        }

        qb.push(
            r#"
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
)AS all_data
GROUP BY all_data.wallet_address,all_data.account_id
)AS all_data2
        "#,
        );

        let res = qb
            .build_query_as::<SumResult>()
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()));

        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            "ApiAssetsDao: get_api_wallet_total_assets_v2"
        );
        res
    }

    pub async fn get_api_wallet_total_assets_v3<'a, E>(
        exec: E,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<Vec<AssetBalanceEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let start = std::time::Instant::now();
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    a.balance,
    a.chain_code,
    a.token_address
FROM api_account acc INDEXED BY api_account_wallet_status_idx
JOIN api_chain c
    ON c.chain_code = acc.chain_code
    AND c.status = 1
JOIN api_assets a INDEXED BY api_assets_join_cover_idx
    ON a.address = acc.address
    AND a.chain_code = acc.chain_code
    AND a.status = 1
WHERE acc.wallet_address =
"#,
        );

        qb.push_bind(wallet_address);
        qb.push(" AND acc.status = 1");
        qb.push(" AND a.balance != '0'");

        if let Some(account_id) = account_id {
            qb.push(" AND acc.account_id = ").push_bind(account_id);
        }
        if let Some(chain_code) = chain_code {
            qb.push(" AND acc.chain_code = ").push_bind(chain_code);
        }

        let res = qb
            .build_query_as::<AssetBalanceEntity>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()));

        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            wallet_address = wallet_address,
            "ApiAssetsDao: get_api_wallet_total_assets_v3"
        );
        res
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
        let start = std::time::Instant::now();
        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
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
"#,
        );

        qb.push(" AND api_account.wallet_address = ").push_bind(wallet_address);
        if let Some(account_id) = account_id {
            qb.push(" AND api_account.account_id = ").push_bind(account_id);
        }
        if let Some(chain_code) = chain_code {
            qb.push(" AND api_account.chain_code = ").push_bind(chain_code);
        }

        qb.push(
            r#"
GROUP BY api_account.wallet_address,api_account.account_id,api_account.chain_code,api_assets.token_address
)AS all_data
where 1=1
"#,
        );
        if hide_zero_balance {
            qb.push(" AND all_data.total_coin_quantity > 0");
        }

        qb.push(
            r#"
GROUP BY all_data.wallet_address,all_data.account_id,all_data.symbol
ORDER BY total_account_amount DESC
        "#,
        );

        let res = qb
            .build_query_as::<ApiAssertSummeryEntity>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()));

        tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            "ApiAssetsDao: get_api_wallet_assets_v2"
        );
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir(prefix: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    fn make_asset(
        address: &str,
        chain_code: &str,
        symbol: &str,
        token_address: &str,
        balance: &str,
    ) -> ApiCreateAssetsVo {
        let assets_id = crate::entities::assets::AssetsId::new(
            address,
            chain_code,
            symbol,
            Some(token_address.to_string()),
        );

        crate::entities::api_assets::ApiCreateAssetsVo::new(assets_id, 18, None, 0)
            .with_name("t")
            .with_balance(balance)
    }

    #[tokio::test]
    async fn init_first_then_sync_updates_balance() {
        let dir = make_temp_dir("wallet_db_api_assets_balance_upsert_1");
        let ctx = crate::SqliteContext::new(&dir, Some("api_wallet.db")).await.unwrap();
        let pool = ctx.get_pool().unwrap();

        let mut conn = pool.acquire().await.unwrap();

        let init_asset = make_asset("addr1", "eth", "ETH", "0xtoken", "0");
        ApiAssetsDao::upsert_assets_multi(&mut *conn, vec![init_asset]).await.unwrap();

        let sync_asset = make_asset("addr1", "eth", "ETH", "0xtoken", "123");
        ApiAssetsDao::upsert_assets_multi_update_balance(&mut *conn, vec![sync_asset])
            .await
            .unwrap();

        let balance: String = sqlx::query_scalar(
            "SELECT balance FROM api_assets WHERE address = ? AND chain_code = ? AND token_address = ?",
        )
        .bind("addr1")
        .bind("eth")
        .bind("0xtoken")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(balance, "123");
    }

    #[tokio::test]
    async fn sync_first_then_init_does_not_overwrite_balance() {
        let dir = make_temp_dir("wallet_db_api_assets_balance_upsert_2");
        let ctx = crate::SqliteContext::new(&dir, Some("api_wallet.db")).await.unwrap();
        let pool = ctx.get_pool().unwrap();

        let mut conn = pool.acquire().await.unwrap();

        let sync_asset = make_asset("addr1", "eth", "ETH", "0xtoken", "123");
        ApiAssetsDao::upsert_assets_multi_update_balance(&mut *conn, vec![sync_asset])
            .await
            .unwrap();

        let init_asset = make_asset("addr1", "eth", "ETH", "0xtoken", "0");
        ApiAssetsDao::upsert_assets_multi(&mut *conn, vec![init_asset]).await.unwrap();

        let balance: String = sqlx::query_scalar(
            "SELECT balance FROM api_assets WHERE address = ? AND chain_code = ? AND token_address = ?",
        )
        .bind("addr1")
        .bind("eth")
        .bind("0xtoken")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(balance, "123");
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

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct AssetBalanceEntity {
    pub balance: String,
    pub chain_code: String,
    pub token_address: String,
}
