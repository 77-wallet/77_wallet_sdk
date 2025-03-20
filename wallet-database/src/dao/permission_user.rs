use crate::entities::permission_user::PermissionUserEntity;
use chrono::SecondsFormat;
use sqlx::{Executor, Sqlite};

pub(crate) struct PermissionUserDao;

impl PermissionUserDao {
    // 批量新增成员
    pub async fn batch_add<'a, E>(
        users: &[PermissionUserEntity],
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = String::from(
            "INSERT INTO permission_user (address,grantor_addr, permission_id, is_self, weight,created_at) VALUES ",
        );

        for (i, param) in users.iter().enumerate() {
            if i != 0 {
                query.push_str(", ");
            }
            query.push_str(&format!(
                "('{}','{}','{}', '{}', {}, '{}')",
                param.address,
                param.grantor_addr,
                param.permission_id,
                param.is_self,
                param.weight,
                param.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            ));
        }

        sqlx::query(&query).execute(exec).await?;

        Ok(())
    }

    pub async fn find_by_permission<'a, E>(
        permission_id: &str,
        exec: E,
    ) -> Result<Vec<PermissionUserEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"select * from permission_user where permission_id = ?"#;

        let result = sqlx::query_as::<_, PermissionUserEntity>(sql)
            .bind(permission_id)
            .fetch_all(exec)
            .await?;

        Ok(result)
    }

    pub async fn self_users<'a, E>(
        permission_id: &str,
        exec: E,
    ) -> Result<Vec<PermissionUserEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"select * from permission_user where permission_id = ? and is_self = 1"#;

        let result = sqlx::query_as::<_, PermissionUserEntity>(sql)
            .bind(permission_id)
            .fetch_all(exec)
            .await?;

        Ok(result)
    }

    pub async fn delete_by_permission<'a, E>(
        permission_id: &str,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"delete from permission_user where permission_id = ?"#;

        sqlx::query(sql).bind(permission_id).execute(exec).await?;

        Ok(())
    }

    pub async fn delete_by_grantor_addr<'a, E>(
        grantor_addr: &str,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"delete from permission_user where grantor_addr = ?"#;

        sqlx::query(sql).bind(grantor_addr).execute(exec).await?;

        Ok(())
    }
}
