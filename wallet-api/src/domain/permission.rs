use wallet_database::{
    entities::{
        account::{self, AccountEntity},
        permission::PermissionEntity,
        permission_user::PermissionUserEntity,
    },
    repositories::permission::PermissionRepo,
    DbPool,
};
use wallet_transport_backend::api::permission::GetPermissionBackReq;
use wallet_types::constant::chain_code;
use wallet_utils::serde_func;

use crate::mqtt::payload::incoming::permission::PermissionAccept;

pub struct PermissionDomain;

impl PermissionDomain {
    pub async fn mark_user_isself(
        pool: &DbPool,
        users: &mut [PermissionUserEntity],
    ) -> Result<(), crate::ServiceError> {
        for user in users.iter_mut() {
            let req = account::QueryReq::new_address_chain(&user.address, chain_code::TRON);
            let account = AccountEntity::detail(pool.as_ref(), &req).await?;
            if account.is_some() {
                user.is_self = 1;
            }
        }
        Ok(())
    }

    // 恢复权限数据
    pub async fn recover_permission(uids: &[&str]) -> Result<(), crate::ServiceError> {
        let bakend = crate::Context::get_global_backend_api()?;

        let aes_cbc_cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let pool = crate::Context::get_global_sqlite_pool()?;

        for uid in uids {
            let req = GetPermissionBackReq {
                address: None,
                uid: Some(uid.to_string()),
            };
            let result = bakend.get_permission_backup(req, aes_cbc_cryptor).await?;

            for item in result.list {
                let permission = serde_func::serde_from_str::<PermissionAccept>(&item.data);

                match permission {
                    Ok(permission) => {
                        if let Err(e) = Self::handel_one_item(&pool, &permission).await {
                            tracing::warn!("[recover_permission] error:{}", e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[recover_permission] parse error:{}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn handel_one_item(
        pool: &DbPool,
        permission: &PermissionAccept,
    ) -> Result<(), crate::ServiceError> {
        // 查询是否存在
        let old_permision = PermissionRepo::find_by_grantor_and_active(
            pool,
            &permission.permission.grantor_addr,
            permission.permission.active_id,
            true,
        )
        .await?;

        let mut users = permission.users.clone();
        Self::mark_user_isself(pool, &mut users).await?;

        let is_update = old_permision.is_some();

        if users.iter().any(|u| u.is_self == 1) {
            if is_update {
                PermissionRepo::upate_with_user(pool, &permission.permission, &users).await?;
            } else {
                PermissionRepo::add_with_user(pool, &permission.permission, &users).await?;
            }
        } else {
            tracing::info!("recover permission -->  skipped: not self {}", is_update);
        }

        Ok(())
    }
}
