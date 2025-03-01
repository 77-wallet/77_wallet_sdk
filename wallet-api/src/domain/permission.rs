use wallet_chain_interact::tron::protocol::account::TronAccount;
use wallet_database::{
    entities::{
        account::{self, AccountEntity},
        permission_user::PermissionUserEntity,
    },
    repositories::permission::PermissionRepo,
    DbPool,
};
use wallet_transport_backend::api::permission::GetPermissionBackReq;
use wallet_types::constant::chain_code;

use crate::mqtt::payload::incoming::permission::NewPermissionUser;

use super::chain::adapter::ChainAdapterFactory;

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
                if let Err(e) = Self::handel_one_item(&pool, &item.data).await {
                    tracing::warn!("[recover_permission] error:{}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn self_contain_permiison(
        pool: &DbPool,
        account: &TronAccount,
        address: &str,
    ) -> Result<Vec<NewPermissionUser>, crate::ServiceError> {
        let addresses = account
            .active_permission
            .iter()
            .flat_map(|p| p.keys.iter().map(|k| k.address.clone()))
            .collect::<Vec<String>>();

        let local_account = AccountEntity::list_in_address(
            pool.as_ref(),
            &addresses,
            Some(chain_code::TRON.to_string()),
        )
        .await?;

        let mut reuslt = vec![];
        for item in account.active_permission.iter() {
            for key in item.keys.iter() {
                if local_account
                    .iter()
                    .find(|a| a.address == key.address)
                    .is_some()
                {
                    let permission_with_user = NewPermissionUser::try_from((item, address))?;

                    reuslt.push(permission_with_user);
                }
            }
        }

        Ok(reuslt)
    }

    pub async fn del_add_update(
        pool: &DbPool,
        mut permissions: Vec<NewPermissionUser>,
        grantor_addr: &str,
    ) -> Result<(), crate::ServiceError> {
        // 标记那些是自己
        for p in permissions.iter_mut() {
            Self::mark_user_isself(pool, &mut p.users).await?;
        }

        let mut p = vec![];
        let mut users = vec![];
        for item in permissions {
            p.push(item.permission);
            users.extend(item.users);
        }

        PermissionRepo::del_add(pool, &p, &users, grantor_addr).await?;

        Ok(())
    }

    async fn handel_one_item(pool: &DbPool, grantor_addr: &str) -> Result<(), crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let account = chain.account_info(grantor_addr).await?;

        let new_permission =
            PermissionDomain::self_contain_permiison(&pool, &account, grantor_addr).await?;

        if new_permission.len() > 0 {
            PermissionDomain::del_add_update(&pool, new_permission, grantor_addr).await?;
        }

        Ok(())
    }
}
