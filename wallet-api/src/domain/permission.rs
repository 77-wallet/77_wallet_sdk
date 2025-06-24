use crate::messaging::mqtt::topics::NewPermissionUser;

use super::{chain::adapter::ChainAdapterFactory, multisig::MultisigQueueDomain};
use wallet_chain_interact::tron::protocol::account::TronAccount;
use wallet_database::{
    entities::{
        account::{self, AccountEntity},
        permission_user::PermissionUserEntity,
    },
    repositories::{multisig_queue::MultisigQueueRepo, permission::PermissionRepo},
    DbPool,
};
use wallet_transport_backend::api::permission::GetPermissionBackReq;
use wallet_types::constant::chain_code;

pub struct PermissionDomain;

impl PermissionDomain {
    pub async fn mark_user_is_self(
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
    pub async fn recover_permission(addresses: Vec<String>) -> Result<(), crate::ServiceError> {
        let backend = crate::Context::get_global_backend_api()?;

        let pool = crate::Context::get_global_sqlite_pool()?;

        for address in addresses {
            let req = GetPermissionBackReq {
                address: Some(address),
                uid: None,
            };
            let result = backend.get_permission_backup(req).await?;

            for item in result.list {
                if let Err(e) = Self::handel_one_item(&pool, &item).await {
                    tracing::warn!("[recover_permission] error:{}", e);
                }
            }
        }

        Ok(())
    }

    // retain the permissions to self.
    pub async fn self_contain_permission(
        pool: &DbPool,
        account: &TronAccount,
        address: &str,
    ) -> Result<Vec<NewPermissionUser>, crate::ServiceError> {
        let mut result = vec![];

        let addresses = account.all_actives_user();

        let chain_code = Some(chain_code::TRON.to_string());
        let local_account =
            AccountEntity::list_in_address(pool.as_ref(), &addresses, chain_code).await?;
        if local_account.is_empty() {
            return Ok(result);
        }

        for item in account.active_permission.iter() {
            // 过滤掉只有一个自己的权限
            if item.keys.len() == 1 && item.keys[0].address == address {
                continue;
            }

            for key in item.keys.iter() {
                if local_account.iter().any(|a| a.address == key.address) {
                    let permission_with_user = NewPermissionUser::try_from((item, address))?;
                    result.push(permission_with_user);
                    break;
                }
            }
        }

        Ok(result)
    }

    // 从account 中找到新增的数据
    pub async fn find_permission(
        account: &TronAccount,
        id: i64,
        address: &str,
    ) -> Result<NewPermissionUser, crate::ServiceError> {
        for item in account.active_permission.iter() {
            if item.id.unwrap_or_default() as i64 == id {
                let permission_with_user = NewPermissionUser::try_from((item, address))?;
                return Ok(permission_with_user);
            }
        }

        Err(crate::BusinessError::Permission(
            crate::PermissionError::ActivesPermissionNotFound,
        ))?
    }

    pub async fn del_add_update(
        pool: &DbPool,
        mut permissions: Vec<NewPermissionUser>,
        grantor_addr: &str,
    ) -> Result<(), crate::ServiceError> {
        // 标记那些是自己
        for p in permissions.iter_mut() {
            Self::mark_user_is_self(pool, &mut p.users).await?;
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
            PermissionDomain::self_contain_permission(pool, &account, grantor_addr).await?;

        if !new_permission.is_empty() {
            PermissionDomain::del_add_update(pool, new_permission, grantor_addr).await?;
        }

        Ok(())
    }

    // 根据地址删除权限(在删除钱包或者删除账号的时候，处理账号状态)
    pub async fn delete_by_address(
        pool: &DbPool,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        // 1.all permission  about the address
        let mut permissions = PermissionRepo::all_permission_with_user(pool, address).await?;

        // 有当前地址的标记为is_self == 0
        permissions.iter_mut().for_each(|permission| {
            permission
                .user
                .iter_mut()
                .filter(|u| u.address == address)
                .for_each(|u| u.is_self = 0)
        });

        // 2. handel permission
        for permission in permissions.iter() {
            if permission.has_self_addr() {
                // 修改成员变量的is_self
                for u in permission.user.iter() {
                    if u.address == address {
                        PermissionRepo::update_self_mark(
                            pool,
                            &permission.permission.grantor_addr,
                            &u.address,
                        )
                        .await?;
                    }
                }
            } else {
                PermissionRepo::delete_all_by_id(pool, &permission.permission.id).await?;
                MultisigQueueRepo::delete_queue_by_permission(pool, &permission.permission.id)
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn queue_fail_and_upload(
        pool: &DbPool,
        grantor_addr: &str,
    ) -> Result<(), crate::ServiceError> {
        let result = MultisigQueueRepo::permission_update_fail(grantor_addr, pool).await?;

        for queue in result {
            MultisigQueueDomain::update_raw_data(&queue.id, pool.clone()).await?;
        }
        Ok(())
    }
}
