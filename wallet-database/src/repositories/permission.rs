use crate::{
    dao::{permission::PermissionDao, permission_user::PermissionUserDao},
    entities::{
        permission::{PermissionEntity, PermissionWithuserEntity},
        permission_user::PermissionUserEntity,
    },
    DbPool,
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

        tx.commit()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

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

        tx.commit()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    // 新增权限以及成员
    pub async fn upate_with_user(
        pool: &DbPool,
        permission: &PermissionEntity,
        users: &[PermissionUserEntity],
    ) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 修改原permission
        PermissionDao::update(&permission, tx.as_mut()).await?;

        // 删除原来的成员
        PermissionUserDao::delete_by_permission(&permission.id, tx.as_mut()).await?;

        // 批量新增
        PermissionUserDao::batch_add(&users, tx.as_mut()).await?;

        tx.commit()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn update_permission(
        pool: &DbPool,
        permission: &PermissionEntity,
    ) -> Result<(), crate::Error> {
        Ok(PermissionDao::update(permission, pool.as_ref()).await?)
    }

    pub async fn permission_with_user(
        pool: &DbPool,
        grantor_addr: &str,
        active_id: i64,
        include_del: bool,
    ) -> Result<Option<PermissionWithuserEntity>, crate::Error> {
        let permission = PermissionDao::find_by_grantor_active(
            grantor_addr,
            active_id,
            include_del,
            pool.as_ref(),
        )
        .await?;

        if let Some(permission) = permission {
            let user = PermissionUserDao::find_by_permission(&permission.id, pool.as_ref()).await?;

            Ok(Some(PermissionWithuserEntity { permission, user }))
        } else {
            Ok(None)
        }
    }

    // 所有的权限
    pub async fn all_permission_with_user(
        pool: &DbPool,
    ) -> Result<Vec<PermissionWithuserEntity>, crate::Error> {
        let permissions = PermissionDao::all_permission(pool.as_ref()).await?;

        let mut result = vec![];
        for permission in permissions {
            let users =
                PermissionUserDao::find_by_permission(&permission.id, pool.as_ref()).await?;

            result.push(PermissionWithuserEntity {
                permission,
                user: users,
            });
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

    pub async fn delete(
        pool: &DbPool,
        grantor_addr: &str,
        active_id: i64,
    ) -> Result<(), crate::Error> {
        Ok(PermissionDao::solf_delete(grantor_addr, active_id, pool.as_ref()).await?)
    }
}
