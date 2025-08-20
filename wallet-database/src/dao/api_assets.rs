use crate::{
    dao::assets::CreateAssetsVo,
    entities::{api_assets::ApiAssetsEntity, assets::AssetsId},
    error::DatabaseError,
    sql_utils::{update_builder::DynamicUpdateBuilder, SqlExecutableNoReturn},
};

use sqlx::{Executor, Sqlite};
use crate::entities::api_assets::ApiCreateAssetsVo;

pub(crate) struct ApiAssetsDao;

impl ApiAssetsDao {
    pub async fn list<'a, E>(exec: E) -> Result<Vec<ApiAssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = String::from("SELECT * FROM api_assets WHERE status = 1");
        let mut conditions = Vec::new();
        conditions.push(
            " EXISTS (
                    SELECT 1
                    FROM chain
                    WHERE chain.chain_code = api_assets.chain_code
                    AND chain.status = 1
                )"
            .to_string(),
        );

        conditions.push(
            " EXISTS (
                    SELECT 1
                    FROM coin
                    WHERE coin.chain_code = api_assets.chain_code
                    AND coin.token_address = api_assets.token_address
                    AND coin.symbol = api_assets.symbol
                    AND coin.status = 1
                )"
            .to_string(),
        );

        sqlx::query_as::<sqlx::Sqlite, ApiAssetsEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_balance<'a, E>(
        exec: E,
        assets_id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let builder = DynamicUpdateBuilder::new("api_assets")
            .set("balance", balance)
            .set("updated_at", "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("address", &assets_id.address)
            .and_where_eq("symbol", &assets_id.symbol)
            .and_where_eq("chain_code", &assets_id.chain_code)
            .and_where_eq(
                "token_address",
                assets_id.token_address.clone().unwrap_or_default(),
            );
        SqlExecutableNoReturn::execute(&builder, exec).await

        // let sql = String::from(
        //     r#"
        //     UPDATE assets SET
        //         balance = ?,
        //         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        //     WHERE address = ?
        //     AND symbol = ?
        //     AND chain_code = ?
        //     AND token_address IS ?
        // "#,
        // );

        // let token_address = assets_id.token_address.clone().unwrap_or_default();
        // let query = sqlx::query(&sql)
        //     .bind(balance)
        //     .bind(assets_id.address.to_string())
        //     .bind(assets_id.symbol.to_string())
        //     .bind(assets_id.chain_code.to_string())
        //     .bind(token_address);

        // query
        //     .execute(exec)
        //     .await
        //     .map(|_| ())
        //     .map_err(|_| crate::Error::Database(DatabaseError::UpdateFailed))
    }

    pub async fn upsert_assets<'a, E>(exec: E, assets: ApiCreateAssetsVo) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let ApiCreateAssetsVo {
            assets_id,
            name,
            decimals,
            protocol,
            status,
            is_multisig,
            balance,
        } = assets;

        let token_address = assets_id.token_address.unwrap_or_default();
        let protocol = protocol.unwrap_or_default();

        let sql = r#"
        INSERT INTO api_assets
        (
            name, symbol, decimals, address, chain_code, token_address, protocol, status, balance, is_multisig, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT (symbol, address, chain_code, token_address)
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

    pub async fn delete_assets<'a, E>(exec: E, assets_id: &AssetsId) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE api_assets 
        SET status = $5, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE address = $1 AND symbol = $2 AND chain_code = $3 AND token_address = $4
            AND EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = api_assets.chain_code
                AND chain.status = 1
            )
            AND EXISTS (
                SELECT 1
                FROM coin
                WHERE coin.chain_code = api_assets.chain_code
                AND coin.token_address = api_assets.token_address
                AND coin.symbol = api_assets.symbol
                AND coin.status = 1
            );
        "#;

        sqlx::query(sql)
            .bind(assets_id.address.to_string())
            .bind(assets_id.symbol.to_string())
            .bind(assets_id.chain_code.to_string())
            .bind(assets_id.token_address.clone())
            .bind(0) // Assuming 0 is the status for deletion
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|_| crate::Error::Database(DatabaseError::UpdateFailed))
    }

    pub async fn delete_multi_assets<'a, E>(
        exec: E,
        assets_ids: Vec<AssetsId>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if assets_ids.is_empty() {
            return Ok(());
        }
        let placeholders = assets_ids
            .iter()
            .map(|_| "(?, ?, ?, ?)")
            .collect::<Vec<_>>()
            .join(", ");

        // 构建 SQL 查询
        let sql = format!(
            "UPDATE api_assets SET status = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE (address, symbol, chain_code, token_address) IN ({})",
            placeholders
        );

        let mut query = sqlx::query(&sql);

        // 绑定参数
        for assets_id in &assets_ids {
            let token_address = match &assets_id.token_address {
                Some(token_address) => token_address.to_string(),
                None => String::new(),
            };
            query = query
                .bind(&assets_id.address)
                .bind(&assets_id.symbol)
                .bind(&assets_id.chain_code)
                .bind(token_address);
        }

        // 执行查询
        query
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

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
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            )
            AND EXISTS (
                SELECT 1
                FROM coin
                WHERE coin.chain_code = assets.chain_code
                AND coin.token_address = assets.token_address
                AND coin.symbol = assets.symbol
                AND coin.status = 1
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
}
