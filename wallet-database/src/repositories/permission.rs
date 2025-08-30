use crate::{
    DbPool,
    dao::{permission::PermissionDao, permission_user::PermissionUserDao},
    entities::{
        permission::{PermissionEntity, PermissionWithUserEntity},
        permission_user::PermissionUserEntity,
    },
};

pub struct PermissionRepo;

impl PermissionRepo {
    // 新增权限以及成员
    pub async fn add_with_user(
        pool: &DbPool,
        permission: &PermissionEntity,
        users: &[PermissionUserEntity],
    ) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        PermissionDao::add(permission, tx.as_mut()).await?;

        PermissionUserDao::batch_add(users, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn del_add(
        pool: &DbPool,
        permissions: &[PermissionEntity],
        users: &[PermissionUserEntity],
        grantor_addr: &str,
    ) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 删除原来的权限
        PermissionDao::delete_by_grantor_addr(grantor_addr, tx.as_mut()).await?;
        // 删除成员
        PermissionUserDao::delete_by_grantor_addr(grantor_addr, tx.as_mut()).await?;

        // 新增权限
        for permission in permissions {
            PermissionDao::add(permission, tx.as_mut()).await?;
        }

        //  新增成员
        PermissionUserDao::batch_add(users, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    // 新增权限以及成员
    pub async fn update_with_user(
        pool: &DbPool,
        permission: &PermissionEntity,
        users: &[PermissionUserEntity],
    ) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 修改原permission
        PermissionDao::update(permission, tx.as_mut()).await?;

        // 删除原来的成员
        PermissionUserDao::delete_by_permission(&permission.id, tx.as_mut()).await?;

        // 批量新增
        PermissionUserDao::batch_add(users, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn update_permission(
        pool: &DbPool,
        permission: &PermissionEntity,
    ) -> Result<(), crate::Error> {
        Ok(PermissionDao::update(permission, pool.as_ref()).await?)
    }

    pub async fn update_self_mark(
        pool: &DbPool,
        grantor_addr: &str,
        address: &str,
    ) -> Result<(), crate::Error> {
        Ok(PermissionUserDao::update_self_mark(grantor_addr, address, pool.as_ref()).await?)
    }

    pub async fn permission_with_user(
        pool: &DbPool,
        grantor_addr: &str,
        active_id: i64,
        include_del: bool,
    ) -> Result<Option<PermissionWithUserEntity>, crate::Error> {
        let permission = PermissionDao::find_by_grantor_active(
            grantor_addr,
            active_id,
            include_del,
            pool.as_ref(),
        )
        .await?;

        if let Some(permission) = permission {
            let user = PermissionUserDao::find_by_permission(&permission.id, pool.as_ref()).await?;

            Ok(Some(PermissionWithUserEntity { permission, user }))
        } else {
            Ok(None)
        }
    }

    // 所有的权限
    pub async fn all_permission_with_user(
        pool: &DbPool,
        user_addr: &str,
    ) -> Result<Vec<PermissionWithUserEntity>, crate::Error> {
        let permissions = PermissionDao::all_permission(pool.as_ref(), user_addr).await?;

        let mut result = vec![];
        for permission in permissions {
            let users =
                PermissionUserDao::find_by_permission(&permission.id, pool.as_ref()).await?;

            result.push(PermissionWithUserEntity { permission, user: users });
        }
        Ok(result)
    }

    pub async fn find_by_grantor_and_active(
        pool: &DbPool,
        grantor_addr: &str,
        active_id: i64,
        include_del: bool,
    ) -> Result<Option<PermissionEntity>, crate::Error> {
        let res = PermissionDao::find_by_grantor_active(
            grantor_addr,
            active_id,
            include_del,
            pool.as_ref(),
        )
        .await?;
        Ok(res)
    }

    // delete permission and user
    pub async fn delete_all(pool: &DbPool, grantor_addr: &str) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // delete permission
        PermissionDao::delete_by_grantor_addr(grantor_addr, tx.as_mut()).await?;

        // delete all users
        PermissionUserDao::delete_by_grantor_addr(grantor_addr, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn delete_all_by_id(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // delete permission
        PermissionDao::delete_by_id(id, tx.as_mut()).await?;

        // delete all users
        PermissionUserDao::delete_by_permission(id, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    // 删除成员以及权限
    pub async fn delete_one(pool: &DbPool, id: &str) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 删除原来的权限
        PermissionDao::delete_by_id(id, tx.as_mut()).await?;
        // 删除成员
        PermissionUserDao::delete_by_permission(id, tx.as_mut()).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<PermissionEntity, crate::Error> {
        let rs = PermissionDao::find_by_id(id, false, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;
        Ok(rs)
    }

    pub async fn find_option(
        pool: &DbPool,
        id: &str,
    ) -> Result<Option<PermissionEntity>, crate::Error> {
        Ok(PermissionDao::find_by_id(id, false, pool.as_ref()).await?)
    }

    pub async fn self_user(
        pool: &DbPool,
        permission_id: &str,
    ) -> Result<Vec<PermissionUserEntity>, crate::Error> {
        Ok(PermissionUserDao::self_users(permission_id, pool.as_ref()).await?)
    }

    pub async fn permission_by_users(
        pool: &DbPool,
        users: &Vec<String>,
    ) -> Result<Vec<PermissionEntity>, crate::Error> {
        Ok(PermissionDao::permission_by_uses(pool.as_ref(), users).await?)
    }
}
