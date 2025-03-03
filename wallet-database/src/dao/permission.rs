use crate::entities::permission::PermissionEntity;
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct PermissionDao;

impl PermissionDao {
    pub async fn add<'a, E>(
        permission: &PermissionEntity,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            INSERT INTO permission
            (id, name,grantor_addr, types, active_id, threshold, member, chain_code, operations, is_del, created_at)
                VALUES
            (?,?, ?, ?, ?, ?, ?, ?, ?, ?,?)"#;

        sqlx::query(&sql)
            .bind(&permission.id)
            .bind(&permission.name)
            .bind(&permission.grantor_addr)
            .bind(&permission.types)
            .bind(&permission.active_id)
            .bind(permission.threshold)
            .bind(permission.member)
            .bind(&permission.chain_code)
            .bind(&permission.operations)
            .bind(permission.is_del)
            .bind(
                permission
                    .created_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            )
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn update<'a, E>(
        permission: &PermissionEntity,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE permission
        SET 
            name = ?,
            threshold = ?, 
            member = ?, 
            chain_code = ?, 
            operations = ?, 
            is_del = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE grantor_addr = ? and active_id = ?
    "#;
        tracing::warn!("{:#?}", permission);

        sqlx::query(&sql)
            .bind(&permission.name)
            .bind(permission.threshold)
            .bind(permission.member)
            .bind(&permission.chain_code)
            .bind(&permission.operations)
            .bind(&permission.is_del)
            .bind(&permission.grantor_addr)
            .bind(&permission.active_id)
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn all_permission<'a, E>(
        exec: E,
    ) -> Result<Vec<PermissionEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from permission where is_del = 0";

        let result = sqlx::query_as::<_, PermissionEntity>(&sql)
            .fetch_all(exec)
            .await?;

        Ok(result)
    }

    // 1 包含删除 0包含
    pub async fn find_by_grantor_active<'a, E>(
        grantor_addr: &str,
        active_id: i64,
        include_del: bool,
        exec: E,
    ) -> Result<Option<PermissionEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = if include_del {
            r#"select * from permission where grantor_addr = ? and active_id = ?"#
        } else {
            r#"select * from permission where grantor_addr = ? and active_id = ? and is_del = 0"#
        };

        let result = sqlx::query_as::<_, PermissionEntity>(&sql)
            .bind(grantor_addr)
            .bind(active_id)
            .bind(include_del)
            .fetch_optional(exec)
            .await?;

        Ok(result)
    }

    pub async fn solf_delete<'a, E>(
        grantor_addr: &str,
        active_id: i64,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"update permission 
            set 
                is_del = 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            where 
                grantor_addr = ? and active_id = ?"#;

        let _c = sqlx::query(&sql)
            .bind(grantor_addr)
            .bind(active_id)
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn delete_by_grantor_addr<'a, E>(
        grantor_addr: &str,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"delete from permission where grantor_addr = ?"#;

        sqlx::query(&sql).bind(grantor_addr).execute(exec).await?;

        Ok(())
    }
}
