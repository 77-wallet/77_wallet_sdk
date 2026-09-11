use crate::{
    entities::{
        asset_token_key::AssetTokenKey,
        assets::{AssetsEntity, AssetsEntityWithAddressType, AssetsId},
    },
    error::database::DatabaseError,
};

pub struct AssetsDao;
use sqlx::{Executor, QueryBuilder, Sqlite};

impl AssetsDao {
    pub async fn list<'a, E>(exec: E) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = String::from("SELECT * FROM assets");
        let mut conditions = Vec::new();
        conditions.push(" status = 1".to_string());
        conditions.push(
            " EXISTS (
                    SELECT 1
                    FROM chain
                    WHERE chain.chain_code = assets.chain_code
                    AND chain.status = 1
                )"
            .to_string(),
        );
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sqlx::query_as::<sqlx::Sqlite, AssetsEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn unactived_list<'a, E>(exec: E) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = String::from("SELECT * FROM assets");
        let mut conditions = Vec::new();
        conditions.push(" status = 0".to_string());

        conditions.push(
            " EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            )"
            .to_string(),
        );

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sqlx::query_as::<sqlx::Sqlite, AssetsEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_coin_assets_in_address<'a, E>(
        exec: E,
        address: Vec<String>,
        status: Option<u8>,
    ) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM assets WHERE ");
        if !address.is_empty() {
            query.push("address IN (");
            let mut values = query.separated(", ");
            for item in &address {
                values.push_bind(item);
            }
            values.push_unseparated(") AND ");
        }

        if let Some(status_value) = status {
            query.push("status = ").push_bind(status_value).push(" AND ");
        }

        query.push(
            " EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            )",
        );

        query
            .build_query_as::<AssetsEntity>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub(crate) async fn get_assets_by_address<'a, E>(
        exec: E,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        token_address: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntityWithAddressType>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        fn push_branch<'q>(
            query: &mut QueryBuilder<'q, Sqlite>,
            base: &'static str,
            addresses: &'q [String],
            chain_code: Option<&'q str>,
            symbol: Option<&'q str>,
            token_address: Option<&'q str>,
            is_multisig: Option<bool>,
            exclude_deleted_account: bool,
        ) {
            query.push(base);
            if !addresses.is_empty() {
                query.push(" AND a.address IN (");
                let mut values = query.separated(", ");
                for address in addresses {
                    values.push_bind(address);
                }
                values.push_unseparated(")");
            }
            if let Some(chain_code) = chain_code {
                query.push(" AND a.chain_code = ").push_bind(chain_code);
            }
            if let Some(symbol) = symbol {
                query.push(" AND a.symbol = ").push_bind(symbol);
            }
            if let Some(token_address) = token_address {
                query.push(" AND a.token_address = ").push_bind(token_address);
            }
            match is_multisig {
                Some(true) => {
                    query.push(" AND a.is_multisig = ").push_bind(1_i8);
                }
                Some(false) => {
                    query.push(" AND a.is_multisig IN (").push_bind(0_i8);
                    query.push(", ").push_bind(2_i8).push(")");
                }
                None => {}
            }
            if exclude_deleted_account {
                query.push(" AND acc.is_del = 0");
            }
        }

        const ACCOUNT_SQL: &str = "SELECT a.name, a.symbol, a.decimals, a.address, a.chain_code,
            a.token_address, a.protocol, a.status, a.balance, a.is_multisig,
            a.created_at, a.updated_at, acc.address_type
            FROM assets AS a JOIN account AS acc
            ON a.address = acc.address AND a.chain_code = acc.chain_code
            WHERE a.status = 1 AND EXISTS (
                SELECT 1 FROM chain
                WHERE chain.chain_code = a.chain_code AND chain.status = 1)";
        const MULTISIG_SQL: &str = "SELECT a.name, a.symbol, a.decimals, a.address, a.chain_code,
            a.token_address, a.protocol, a.status, a.balance, a.is_multisig,
            a.created_at, a.updated_at, acc.address_type
            FROM assets AS a JOIN multisig_account AS acc
            ON a.address = acc.address AND a.chain_code = acc.chain_code
            WHERE a.status = 1 AND EXISTS (
                SELECT 1 FROM chain
                WHERE chain.chain_code = a.chain_code AND chain.status = 1)";

        let mut query = QueryBuilder::<Sqlite>::new("");
        match is_multisig {
            Some(true) => push_branch(
                &mut query,
                MULTISIG_SQL,
                &address,
                chain_code.as_deref(),
                symbol,
                token_address,
                is_multisig,
                true,
            ),
            Some(false) => push_branch(
                &mut query,
                ACCOUNT_SQL,
                &address,
                chain_code.as_deref(),
                symbol,
                token_address,
                is_multisig,
                false,
            ),
            None => {
                push_branch(
                    &mut query,
                    ACCOUNT_SQL,
                    &address,
                    chain_code.as_deref(),
                    symbol,
                    token_address,
                    is_multisig,
                    false,
                );
                query.push(" UNION ");
                push_branch(
                    &mut query,
                    MULTISIG_SQL,
                    &address,
                    chain_code.as_deref(),
                    symbol,
                    token_address,
                    is_multisig,
                    false,
                );
            }
        }

        query
            .build_query_as::<AssetsEntityWithAddressType>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol<'a, E>(
        exec: E,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT * FROM assets
        WHERE status = 1
            AND EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            )",
        );

        if !address.is_empty() {
            query.push(" AND address IN (");
            let mut values = query.separated(", ");
            for item in &address {
                values.push_bind(item);
            }
            values.push_unseparated(")");
        }

        if let Some(code) = chain_code.as_deref() {
            query.push(" AND chain_code = ").push_bind(code);
        }

        if let Some(symbol) = symbol {
            query.push(" AND symbol = ").push_bind(symbol);
        }

        if let Some(is_multisig) = is_multisig {
            if is_multisig {
                query.push(" AND is_multisig = ").push_bind(1_i8);
            } else {
                query.push(" AND is_multisig IN (").push_bind(0_i8);
                query.push(", ").push_bind(2_i8).push(")");
            }
        }

        query
            .build_query_as::<AssetsEntity>()
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // 获取资产
    pub async fn assets_by_id<'a, E>(
        exec: E,
        assets_id: &AssetsId,
    ) -> Result<Option<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM 
                assets
            WHERE status = 1 AND address =$1 AND chain_code = $2 AND token_address = $3
                AND EXISTS (
                    SELECT 1
                    FROM chain
                    WHERE chain.chain_code = assets.chain_code
                    AND chain.status = 1
                );"#;

        let rs = sqlx::query_as::<sqlx::Sqlite, AssetsEntity>(sql)
            .bind(assets_id.address.clone())
            .bind(assets_id.chain_code.clone())
            .bind(assets_id.token_address.as_db_str())
            .fetch_optional(exec)
            .await;

        match rs {
            Ok(rs) => Ok(rs),
            Err(_e) => Err(crate::Error::Database(DatabaseError::QueryFailed)),
        }
    }

    pub async fn list_by_chain_token_map_batch<'a, E>(
        exec: E,
        chain_list: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if chain_list.is_empty() {
            return Ok(vec![]);
        }

        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        for (chain_code, token_address) in chain_list {
            where_parts.push("(chain_code = ? AND token_address = ?)".to_string());
            params.push(chain_code.clone());
            params.push(token_address.clone());
        }

        let where_clause = where_parts.join(" OR ");
        let sql = format!("SELECT * FROM assets WHERE ({})", where_clause);

        let mut query = sqlx::query_as::<_, AssetsEntity>(&sql);
        for param in params {
            query = query.bind(param);
        }

        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_by_addr_token<'a, E>(
        exec: E,
        chain_code: &str,
        token_address: &str,
        address: &str,
    ) -> Result<Option<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT 
                name, symbol,decimals, address, chain_code, token_address, protocol, status, balance,is_multisig,created_at, updated_at
            FROM 
                assets
            WHERE status = 1 AND chain_code =$1 AND token_address = $2 AND address = $3
                AND EXISTS (
                    SELECT 1
                    FROM chain
                    WHERE chain.chain_code = assets.chain_code
                    AND chain.status = 1
                );"#;

        let rs = sqlx::query_as::<sqlx::Sqlite, AssetsEntity>(sql)
            .bind(chain_code)
            .bind(token_address)
            .bind(address)
            .fetch_optional(exec)
            .await;

        match rs {
            Ok(rs) => Ok(rs),
            Err(_e) => Err(crate::Error::Database(DatabaseError::QueryFailed)),
        }
    }

    // 更新余额
    pub async fn update_balance<'a, E>(
        exec: E,
        assets_id: &AssetsId,
        balance: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = String::from(
            r#"
            UPDATE assets SET 
                balance = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE address = ?
            AND chain_code = ?
            AND token_address = ?
        "#,
        );

        let query = sqlx::query(&sql)
            .bind(balance)
            .bind(assets_id.address.to_string())
            .bind(assets_id.chain_code.to_string())
            .bind(&assets_id.token_address);

        query
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
    }

    // 设置多签标识
    pub async fn update_is_multisig<'a, E>(
        exec: E,
        assets_id: &AssetsId,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE assets SET 
            is_multisig = 1,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE address = $1 AND chain_code = $2 AND token_address = $3;
    "#;

        sqlx::query(sql)
            .bind(assets_id.address.to_string())
            .bind(assets_id.chain_code.to_string())
            .bind(&assets_id.token_address)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
    }

    // 插入或更新资产信息
    pub async fn upsert_assets<'a, E>(exec: E, assets: CreateAssetsVo) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let CreateAssetsVo {
            assets_id,
            symbol,
            name,
            decimals,
            protocol,
            status,
            is_multisig,
            balance,
        } = assets;

        let protocol = protocol.unwrap_or_default();

        let sql = r#"
        INSERT INTO assets
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
            .bind(symbol)
            .bind(decimals)
            .bind(assets_id.address)
            .bind(assets_id.chain_code)
            .bind(assets_id.token_address)
            .bind(protocol)
            .bind(status)
            .bind(balance)
            .bind(is_multisig)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
    }

    pub async fn all_assets<'a, E>(
        exec: E,
        addr: Vec<String>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT * FROM assets WHERE status = 1 AND EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            )",
        );

        if let Some(chain_code) = chain_code {
            query.push(" AND chain_code = ").push_bind(chain_code);
        }
        if let Some(keyword) = keyword {
            query.push(" AND symbol LIKE ").push_bind(format!("%{keyword}%"));
        }

        if let Some(is_multisig) = is_multisig {
            if is_multisig {
                query.push(" AND is_multisig = ").push_bind(1_i8);
            } else {
                query.push(" AND is_multisig IN (").push_bind(0_i8);
                query.push(", ").push_bind(2_i8).push(")");
            }
        }

        if !addr.is_empty() {
            query.push(" AND address IN (");
            let mut values = query.separated(", ");
            for address in &addr {
                values.push_bind(address);
            }
            values.push_unseparated(")");
        }

        let result = query.build_query_as::<AssetsEntity>().fetch_all(exec).await;

        result.map_err(|e| crate::Error::Database(e.into()))
    }

    // 删除单个资产
    pub async fn delete_assets<'a, E>(exec: E, assets_id: &AssetsId) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE assets 
        SET status = $4, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE address = $1 AND chain_code = $2 AND token_address = $3
            AND EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            );
    "#;

        sqlx::query(sql)
            .bind(assets_id.address.to_string())
            .bind(assets_id.chain_code.to_string())
            .bind(assets_id.token_address.clone())
            .bind(0) // Assuming 0 is the status for deletion
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))
    }

    pub async fn update_status<'a, E>(
        exec: E,
        chain_code: &str,
        symbol: &str,
        token_address: AssetTokenKey,
        status: u8,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE assets
        SET status = $4, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE chain_code = $1 AND LOWER(symbol) = LOWER($2) AND token_address = $3
            AND EXISTS (
                SELECT 1
                FROM chain
                WHERE chain.chain_code = assets.chain_code
                AND chain.status = 1
            );"#;

        sqlx::query(sql)
            .bind(chain_code)
            .bind(symbol)
            .bind(token_address)
            .bind(status)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(DatabaseError::UpdateFailed(e.to_string())))?;

        Ok(())
    }

    // 删除多个资产
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
        let placeholders = assets_ids.iter().map(|_| "(?, ?, ?)").collect::<Vec<_>>().join(", ");

        // 构建 SQL 查询
        let sql = format!(
            "UPDATE assets SET status = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE (address, chain_code, token_address) IN ({})",
            placeholders
        );

        let mut query = sqlx::query(&sql);

        // 绑定参数
        for assets_id in &assets_ids {
            query = query
                .bind(&assets_id.address)
                .bind(&assets_id.chain_code)
                .bind(&assets_id.token_address);
        }

        // 执行查询
        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_tron_multisig_assets<'a, E>(
        address: &str,
        chain_code: &str,
        is_multisig: i8,
        exec: E,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE assets 
            SET is_multisig = ?, 
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE address = ? AND chain_code = ?;
        "#;

        sqlx::query(sql)
            .bind(is_multisig)
            .bind(address)
            .bind(chain_code)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(crate::DatabaseError::UpdateFailed(e.to_string())))
    }

    pub async fn error_wsol_assets<'a, E>(exec: E) -> Result<Vec<AssetsEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from assets where symbol = 'wSOL' and chain_code = 'sol' and token_address = 'So11111111111111111111111111111111111111112'".to_string();

        let query = sqlx::query_as::<_, AssetsEntity>(&sql);
        let result = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result)
    }

    pub async fn delete_error_wsol_assets<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "delete from assets where symbol = 'wSOL' and chain_code = 'sol' and token_address = 'So11111111111111111111111111111111111111112'".to_string();

        let _c =
            sqlx::query(&sql).execute(exec).await.map(|_| ()).map_err(|e| {
                crate::Error::Database(crate::DatabaseError::UpdateFailed(e.to_string()))
            });
        Ok(())
    }
}

// 创建的账单的类型
#[derive(Debug)]
pub struct CreateAssetsVo {
    pub assets_id: AssetsId,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub protocol: Option<String>,
    pub status: u8,
    pub is_multisig: i32,
    pub balance: String,
}

impl CreateAssetsVo {
    pub fn new(
        assets_id: AssetsId,
        symbol: &str,
        decimals: u8,
        protocol: Option<String>,
        is_multisig: i32,
    ) -> Self {
        Self {
            assets_id,
            symbol: symbol.to_string(),
            name: "name".to_string(),
            decimals,
            protocol,
            status: 1,
            is_multisig,
            balance: "0.00".to_string(),
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_status(mut self, status: u8) -> Self {
        self.status = status;
        self
    }

    pub fn with_u256(
        mut self,
        balance: alloy::primitives::U256,
        decimals: u8,
    ) -> Result<Self, crate::Error> {
        let balance = wallet_utils::unit::format_to_string(balance, decimals)?;
        let balance = wallet_utils::parse_func::decimal_from_str(&balance)?;

        self.balance = balance.to_string();
        Ok(self)
    }

    pub fn with_balance(mut self, balance: &str) -> Self {
        self.balance = balance.to_string();
        self
    }

    pub fn with_protocol(mut self, protocol: Option<String>) -> Self {
        self.protocol = protocol;
        self
    }
}
