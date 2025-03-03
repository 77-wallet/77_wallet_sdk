use super::BackendApi;

// 权限变更请求参数
#[derive(serde::Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAcceptReq {
    pub hash: String,
    pub grantor_addr: String,
    pub sender_user: Vec<String>,
    pub new_user: Vec<String>,
    pub current: CurrentPemission,
}

#[derive(serde::Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CurrentPemission {
    // 上次的成员
    pub original_user: Vec<String>,
    // 修改后的成员
    pub new_user: Vec<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub types: String,
    pub active_id: i64,
    pub opreatins: String,
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetPermissionBackReq {
    pub address: Option<String>,
    pub uid: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PermissionBackupResp {
    pub list: Vec<PermissionItem>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PermissionItem {
    pub data: String,
}

impl BackendApi {
    pub async fn permission_accept(
        &self,
        req: PermissionAcceptReq,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<(), crate::Error> {
        let endpoint = "permission/change";

        let _result = self
            .post_request::<_, bool>(endpoint, &req, aes_cbc_cryptor)
            .await;

        Ok(())
    }

    pub async fn get_permission_backup(
        &self,
        req: GetPermissionBackReq,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<PermissionBackupResp, crate::Error> {
        let endpoint = "permission/getBackUpData";

        let result = self
            .post_request::<_, PermissionBackupResp>(endpoint, &req, aes_cbc_cryptor)
            .await?;

        Ok(result)
    }
}
