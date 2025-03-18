use super::NotificationType;
use wallet_chain_interact::tron::operations::permisions::PermissionTypes;
use wallet_database::entities::permission::PermissionEntity;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionChange {
    // 授权方的地址
    pub grantor_addr: String,
    // 操作类型 (new,update,delete)
    pub types: String,
    // 对应的权限码
    pub operations: Vec<i8>,
    // 通知类型 PermissionChange
    pub notification_type: NotificationType,
}

impl TryFrom<&PermissionEntity> for PermissionChange {
    type Error = crate::ServiceError;
    fn try_from(value: &PermissionEntity) -> Result<Self, crate::ServiceError> {
        Ok(PermissionChange {
            grantor_addr: value.grantor_addr.to_owned(),
            types: "new".to_string(),
            operations: PermissionTypes::from_hex(&value.operations)?,
            notification_type: NotificationType::PermissionChange,
        })
    }
}
