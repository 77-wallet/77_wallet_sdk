use crate::dao::Dao;
use crate::entities::api_withdraw::ApiWithdrawEntity;
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWithdrawDao;

#[async_trait::async_trait]
impl Dao for ApiWithdrawDao {
    type Input = ApiWithdrawEntity;
    type Output = ();
    type Error = crate::Error;

    async fn upsert<'c, E>(executor: E, input: Self::Input) -> Result<Self::Output, Self::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_withdraws
                (id,uid,name,from_addr,to_addr,value,decimals,token_addr,symbol,trade_no,trade_type,status,created_at,updated_at)
            values
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (trade_no)
            do update set
                status = excluded.status,
                updated_at = excluded.updated_at
            returning *
        "#;

        let mut rec = sqlx::query_as::<_, Self::Input>(sql)
            .bind(&input.uid)
            .bind(&input.name)
            .bind(&input.from_addr)
            .bind(&input.to_addr)
            .bind(&input.value)
            .bind(&input.decimals)
            .bind(&input.token_addr)
            .bind(&input.symbol)
            .bind(&input.trade_no)
            .bind(&input.trade_type)
            .bind(&input.status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    async fn list<'c, E>(executor: E) -> Result<Vec<Self::Output>, Self::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        todo!()
    }
}

impl ApiWithdrawDao {
    pub async fn add<'a, E>(
        api_withdraw: &ApiWithdrawEntity,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO api_withdraws
                (id,uid,name,from_addr,to_addr,value,decimals,token_addr,symbol,trade_no,trade_type,status,created_at)
            VALUES
                (?,?, ?, ?, ?, ?, ?, ?, ?, ?,?)
        "#;

        sqlx::query(sql)
            .bind(&api_withdraw.id)
            .bind(&api_withdraw.uid)
            .bind(&api_withdraw.name)
            .bind(&api_withdraw.from_addr)
            .bind(&api_withdraw.to_addr)
            .bind(&api_withdraw.value)
            .bind(&api_withdraw.decimals)
            .bind(&api_withdraw.token_addr)
            .bind(&api_withdraw.symbol)
            .bind(&api_withdraw.trade_no)
            .bind(&api_withdraw.trade_type)
            .bind(
                api_withdraw
                    .created_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            )
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn update<'a, E>(
        api_withdraw: &ApiWithdrawEntity,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE api_withdraws
            SET
                name = ?,
                threshold = ?,
                member = ?,
                chain_code = ?,
                operations = ?,
                is_del = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE trade_no = ?
        "#;
        tracing::warn!("{:#?}", api_withdraw);

        sqlx::query(sql)
            .bind(&api_withdraw.status)
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn all_permission<'a, E>(
        exec: E,
        user_addr: &str,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"SELECT * FROM api_withdraws p WHERE EXISTS
        (
            SELECT 1 FROM permission_user u WHERE u.permission_id = p.id AND u.address = ?
        ) and p.is_del = 0;"#;

        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(user_addr)
            .fetch_all(exec)
            .await?;

        Ok(result)
    }

    pub async fn permission_by_uses<'a, E>(
        exec: E,
        users: &Vec<String>,
    ) -> Result<Vec<ApiWithdrawEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // 如果没有用户，直接返回空向量
        if users.is_empty() {
            return Ok(Vec::new());
        }

        // 根据用户数量构造占位符,
        let placeholders = users.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT * FROM permission p WHERE EXISTS (
            SELECT 1 FROM permission_user u
            WHERE u.permission_id = p.id
              AND u.address in ({})
        ) AND p.is_del = 0;",
            placeholders
        );

        // 创建查询并依次绑定每个用户地址
        let mut query = sqlx::query_as::<_, ApiWithdrawEntity>(&sql);
        for user in users {
            query = query.bind(user.clone());
        }

        let result = query.fetch_all(exec).await?;
        Ok(result)
    }

    // 1 包含删除 0包含
    pub async fn find_by_grantor_active<'a, E>(
        grantor_addr: &str,
        active_id: i64,
        include_del: bool,
        exec: E,
    ) -> Result<Option<ApiWithdrawEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = if include_del {
            r#"select * from permission where grantor_addr = ? and active_id = ?"#
        } else {
            r#"select * from permission where grantor_addr = ? and active_id = ? and is_del = 0"#
        };

        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(grantor_addr)
            .bind(active_id)
            .bind(include_del)
            .fetch_optional(exec)
            .await?;

        Ok(result)
    }

    pub async fn find_by_id<'a, E>(
        id: &str,
        include_del: bool,
        exec: E,
    ) -> Result<Option<ApiWithdrawEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = if include_del {
            r#"select * from permission where id = ?"#
        } else {
            r#"select * from permission where id = ? and is_del = 0"#
        };

        let result = sqlx::query_as::<_, ApiWithdrawEntity>(sql)
            .bind(id)
            .bind(include_del)
            .fetch_optional(exec)
            .await?;

        Ok(result)
    }
}
