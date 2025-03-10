use super::ReturnType;
use crate::{
    request::permission::PermissionReq,
    response_vo::{
        permssion::{AccountPermission, ManagerPermissionResp, PermissionList},
        EstimateFeeResp,
    },
    service::permission::PermssionService,
};

impl crate::WalletManager {
    // all permission list
    pub fn permission_list(&self) -> ReturnType<PermissionList> {
        PermssionService::permission_list().into()
    }

    // trans permission
    pub fn permssion_trans(&self) -> ReturnType<Vec<i8>> {
        PermssionService::permssion_trans().into()
    }

    // account permission
    pub async fn account_permission(&self, address: String) -> ReturnType<AccountPermission> {
        PermssionService::new()
            .await?
            .account_permission(address)
            .await?
            .into()
    }

    // 管理其账号的权限
    pub async fn manager_permission(&self) -> ReturnType<Vec<ManagerPermissionResp>> {
        PermssionService::new()
            .await?
            .manager_permision()
            .await?
            .into()
    }

    pub async fn modify_permission_fee(
        &self,
        req: PermissionReq,
        types: String,
    ) -> ReturnType<EstimateFeeResp> {
        PermssionService::new()
            .await?
            .modify_permission_fee(req, types)
            .await?
            .into()
    }

    pub async fn modify_permission(
        &self,
        req: PermissionReq,
        types: String,
        password: String,
    ) -> ReturnType<String> {
        PermssionService::new()
            .await?
            .modify_permission(req, types, password)
            .await?
            .into()
    }

    pub async fn build_multisig_queue(
        &self,
        req: PermissionReq,
        types: String,
        password: String,
        expiration: i64,
    ) -> ReturnType<String> {
        PermssionService::new()
            .await?
            .build_multisig_permission(req, types, expiration, password)
            .await?
            .into()
    }
}
