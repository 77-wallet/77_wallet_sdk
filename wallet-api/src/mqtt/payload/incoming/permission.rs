use crate::{
    domain::{chain::adapter::ChainAdapterFactory, permission::PermissionDomain},
    request::permission::PermissionReq,
};
use wallet_chain_interact::tron::{
    operations::multisig::Permission, protocol::account::TronAccount,
};
use wallet_database::{
    entities::{
        permission::{PermissionEntity, PermissionWithuserEntity},
        permission_user::PermissionUserEntity,
    },
    repositories::{multisig_queue::MultisigQueueRepo, permission::PermissionRepo},
    DbPool,
};
use wallet_types::constant::chain_code;
use wallet_utils::serde_func;

// biz_type = PERMISSION_ACCEPT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAccept {
    pub grantor_addr: String,
    // 当前操作权限
    pub current: CurrentOp,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CurrentOp {
    // 上次的成员
    original_user: Vec<String>,
    // 修改后的成员
    new_user: Vec<String>,
    name: String,
    #[serde(rename = "type")]
    types: String,
    active_id: i64,
    operations: String,
}

impl PermissionAccept {
    pub fn to_json_val(&self) -> Result<serde_json::Value, crate::ServiceError> {
        Ok(serde_func::serde_to_value(&self)?)
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct NewPermissionUser {
    pub permission: PermissionEntity,
    pub users: Vec<PermissionUserEntity>,
}

// 权限转换为数据库对应的实体
impl TryFrom<(&Permission, &str)> for NewPermissionUser {
    type Error = crate::ServiceError;

    fn try_from(value: (&Permission, &str)) -> Result<Self, Self::Error> {
        let permission = value.0;

        let id = PermissionEntity::get_id(value.1, permission.id.unwrap_or_default() as i64);
        let time = wallet_utils::time::now();

        let p = PermissionEntity {
            id,
            name: permission.permission_name.clone(),
            grantor_addr: value.1.to_string(),
            types: "active".to_string(),
            active_id: permission.id.unwrap_or_default() as i64,
            threshold: permission.threshold as i64,
            member: permission.keys.len() as i64,
            chain_code: chain_code::TRON.to_string(),
            operations: permission.operations.clone().unwrap_or_default().clone(),
            is_del: 0,
            created_at: time.clone(),
            updated_at: None,
        };

        let mut users = vec![];

        for key in permission.keys.iter() {
            let user = PermissionUserEntity {
                id: None,
                address: key.address.clone(),
                grantor_addr: value.1.to_string(),
                permission_id: p.id.clone(),
                is_self: 0,
                weight: key.weight as i64,
                created_at: time.clone(),
                updated_at: None,
            };
            users.push(user);
        }

        Ok(NewPermissionUser {
            permission: p,
            users,
        })
    }
}

impl PermissionAccept {
    pub async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let pool = crate::Context::get_global_sqlite_pool()?;
        let account = chain.account_info(&self.grantor_addr).await?;

        // 判断当前的事件是否是删除(删除需要同步所有的权限数据)
        if self.current.types == PermissionReq::DELETE {
            self.recover_all_old_permission(pool.clone(), &account)
                .await?;

            MultisigQueueRepo::permision_update_fail(&self.grantor_addr, &pool).await?;
        } else {
            let address = &self.grantor_addr;
            let permissions =
                PermissionDomain::find_permission(&account, self.current.active_id, address)
                    .await?;

            self.upsert(pool.clone(), permissions).await?
        }

        // TODO 系统通知

        Ok(())
    }

    // 删除事件、全部更新本地的权限
    async fn recover_all_old_permission(
        &self,
        pool: DbPool,
        account: &TronAccount,
    ) -> Result<(), crate::ServiceError> {
        // 权限是否包含自己
        let new_permission =
            PermissionDomain::self_contain_permiison(&pool, &account, &self.grantor_addr).await?;

        if new_permission.len() > 0 {
            // 删除原来的,新增
            tracing::warn!("recover_all_old_permission :new");
            PermissionDomain::del_add_update(&pool, new_permission, &self.grantor_addr).await?;
        } else {
            tracing::warn!("recover_all_old_permission :delete");
            // 删除原来所有的权限
            PermissionRepo::delete_all(&pool, &self.grantor_addr).await?;
        }

        Ok(())
    }

    // 新增或者更新权限
    async fn upsert(
        &self,
        pool: DbPool,
        permissions: NewPermissionUser,
    ) -> Result<(), crate::ServiceError> {
        // 查询出原来的权限
        let old_permisson = PermissionRepo::permission_with_user(
            &pool,
            &permissions.permission.grantor_addr,
            permissions.permission.active_id,
            true,
        )
        .await?;

        // 存在走更新流程
        if let Some(old_permission) = old_permisson {
            self.update(pool, permissions, old_permission).await
        } else {
            tracing::warn!("add new permission");
            let mut users = permissions.users.clone();

            PermissionDomain::mark_user_isself(&pool, &mut users).await?;
            if users.iter().any(|u| u.is_self == 1) {
                return Ok(
                    PermissionRepo::add_with_user(&pool, &permissions.permission, &users).await?,
                );
            }
            Ok(())
        }
    }

    async fn update(
        &self,
        pool: DbPool,
        permissions: NewPermissionUser,
        old_permission: PermissionWithuserEntity,
    ) -> Result<(), crate::ServiceError> {
        // 是否成员发生了变化
        if old_permission.user_has_changed(&permissions.users) {
            tracing::warn!("update user ");
            self.udpate_user_change(pool.clone(), &permissions, &old_permission.permission.id)
                .await?;
        } else {
            tracing::warn!("only update permisson");
            PermissionRepo::update_permission(&pool, &permissions.permission).await?;
        }

        MultisigQueueRepo::permision_update_fail(&self.grantor_addr, &pool).await?;
        Ok(())
    }

    async fn udpate_user_change(
        &self,
        pool: DbPool,
        permissions: &NewPermissionUser,
        id: &str,
    ) -> Result<(), crate::ServiceError> {
        let mut users = permissions.users.clone();
        PermissionDomain::mark_user_isself(&pool, &mut users).await?;

        // 成员变化是否把自己移除了(没有is_self = 1的数据)
        if users.iter().any(|u| u.is_self == 1) {
            tracing::warn!("change user ");
            PermissionRepo::upate_with_user(&pool, &permissions.permission, &users).await?;
        } else {
            tracing::warn!(" not self delete ");
            PermissionRepo::delete_one(&pool, id).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{mqtt::payload::incoming::permission::PermissionAccept, test::env::get_manager};

    #[tokio::test]
    async fn new_permision() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await?;

        let str = r#"{"grantorAddr":"TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR","current":{"originalUser":["TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1"],"newUser":[],"name":"修改权限","type":"delete","activeId":0,"operations":""}}"#;

        let change = serde_json::from_str::<PermissionAccept>(&str).unwrap();

        let res = change.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }
}
