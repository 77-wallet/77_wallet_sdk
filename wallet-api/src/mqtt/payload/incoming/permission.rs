use crate::domain::permission::PermissionDomain;
use wallet_database::{
    entities::{
        permission::{PermissionEntity, PermissionWithuserEntity},
        permission_user::PermissionUserEntity,
    },
    repositories::permission::PermissionRepo,
    DbPool,
};

// biz_type = PERMISSION_ACCEPT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAccept {
    pub op_type: String,
    pub permission: PermissionEntity,
    pub permission_user: Vec<PermissionUserEntity>,
}

impl PermissionAccept {
    pub async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        // 根据类型区分是删除还是修改权限
        match self.op_type.as_ref() {
            "delete" => self.delete_permission(pool.clone()).await?,
            "upsert" => self.upsert(pool.clone()).await?,
            _ => panic!(),
        }

        // TODO 系统通知

        Ok(())
    }

    // 新增或者更新权限
    async fn upsert(&self, pool: DbPool) -> Result<(), crate::ServiceError> {
        // 查询出原来的权限
        let old_permisson = PermissionRepo::permission_with_user(
            &pool,
            &self.permission.grantor_addr,
            self.permission.active_id,
            true,
        )
        .await?;

        // 存在走更新流程
        if let Some(old_permission) = old_permisson {
            self.update(pool, old_permission).await
        } else {
            let mut users = self.permission_user.clone();

            PermissionDomain::mark_user_isself(&pool, &mut users).await?;

            Ok(PermissionRepo::add_with_user(&pool, &self.permission, &users).await?)
        }
    }

    async fn update(
        &self,
        pool: DbPool,
        old_permission: PermissionWithuserEntity,
    ) -> Result<(), crate::ServiceError> {
        // 是否成员发生了变化
        if old_permission.user_has_changed(&self.permission_user) {
            tracing::warn!("update user ");
            self.udpate_user_change(pool.clone()).await?;
        } else {
            tracing::warn!("only update permisson");
            PermissionRepo::update_permission(&pool, &self.permission).await?;
        }

        // TODO 是否需要把进行中的队列删除
        Ok(())
    }

    async fn udpate_user_change(&self, pool: DbPool) -> Result<(), crate::ServiceError> {
        let mut users = self.permission_user.clone();
        PermissionDomain::mark_user_isself(&pool, &mut users).await?;

        // 成员变化是否把自己移除了(没有is_self = 1的数据)
        if users.iter().any(|u| u.is_self == 1) {
            PermissionRepo::upate_with_user(&pool, &self.permission, &users).await?;
        } else {
            PermissionRepo::delete(
                &pool,
                &self.permission.grantor_addr,
                self.permission.active_id,
            )
            .await?;
        }
        Ok(())
    }

    async fn delete_permission(&self, pool: DbPool) -> Result<(), crate::ServiceError> {
        PermissionRepo::delete(
            &pool,
            &self.permission.grantor_addr,
            self.permission.active_id,
        )
        .await?;
        // TODO 是否需要把进行中的队列删除

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{mqtt::payload::incoming::permission::PermissionAccept, test::env::get_manager};
    use wallet_database::entities::{
        permission::PermissionEntity, permission_user::PermissionUserEntity,
    };

    fn get_data() -> PermissionAccept {
        let permission: PermissionEntity = PermissionEntity {
            id: wallet_utils::snowflake::gen_hash_uid(vec![
                "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR",
                "2",
            ]),
            name: "test_permission".to_string(),
            grantor_addr: "TUe3T6ErJvnoHMQwVrqK246MWeuCEBbyuR".to_string(),
            types: "active".to_string(),
            active_id: 2,
            threshold: 2,
            memeber: 2,
            chain_code: "tron".to_string(),
            operations: "xxx".to_string(),
            is_del: 0,
            created_at: wallet_utils::time::now(),
            updated_at: None,
        };

        let user1 = PermissionUserEntity {
            id: None,
            address: "TNPTj8Dbba6YxW5Za6tFh6SJMZGbUyucXQ".to_string(),
            permission_id: permission.id.clone(),
            is_self: 0,
            weight: 1,
            created_at: wallet_utils::time::now(),
            updated_at: None,
        };

        let user2 = PermissionUserEntity {
            id: None,
            address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            permission_id: permission.id.clone(),
            is_self: 0,
            weight: 1,
            created_at: wallet_utils::time::now(),
            updated_at: None,
        };

        let result = PermissionAccept {
            op_type: "upsert".to_string(),
            permission,
            permission_user: vec![user1, user2],
        };
        result
    }

    #[tokio::test]
    async fn new_permision() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await?;

        let change = get_data();

        let res = change.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }

    #[tokio::test]
    async fn del_permision() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await?;

        let mut change = get_data();
        change.op_type = "delete".to_string();

        let res = change.exec("1").await;
        println!("{:?}", res);
        Ok(())
    }

    #[tokio::test]
    async fn update_permission() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await?;

        let mut change = get_data();
        change.op_type = "upsert".to_string();
        change.permission.operations = "hello world".to_string();
        change.permission.threshold = 5;

        let res = change.exec("1").await;
        tracing::info!("{:?}", res);
        Ok(())
    }

    #[tokio::test]
    // 删除一个成员，自己还在里面
    async fn update_permission1() {
        wallet_utils::init_test_log();
        let (_, _) = get_manager().await.unwrap();

        let mut change = get_data();
        change.permission_user.remove(0);

        let res = change.exec("1").await;
        tracing::info!("{:?}", res);
    }
}
