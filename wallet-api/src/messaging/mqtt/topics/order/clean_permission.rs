// 清空权限事件
use crate::domain::permission::PermissionDomain;
use wallet_database::repositories::permission::PermissionRepo;

// biz_type = CLEAN_PERMISSION
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CleanPermission {
    pub grantor_addr: String,
}

impl CleanPermission {
    fn name(&self) -> String {
        "CLEAN_PERMISSION".to_string()
    }

    pub async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let event_name = self.name();
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Clean Permission");

        // 删除权限
        PermissionRepo::delete_all(&pool, &self.grantor_addr).await?;

        // 更新队列数据
        PermissionDomain::queue_fail_and_upload(&pool, &self.grantor_addr).await?;
        Ok(())
    }
}
