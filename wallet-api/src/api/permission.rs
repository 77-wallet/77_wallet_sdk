use super::ReturnType;
use crate::{
    request::permission::PermissionReq,
    response_vo::{
        permssion::{AccountPermission, PermissionList},
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
    pub async fn manager_permission(&self, address: String) -> ReturnType<AccountPermission> {
        PermssionService::new()
            .await?
            .account_permission(address)
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

    // // new permisson
    // pub async fn add_permission(&self, req: PermissionReq, password: String) -> ReturnType<String> {
    //     PermssionService::new()
    //         .await?
    //         .add_permission(req, password)
    //         .await?
    //         .into()
    // }

    // pub async fn add_permission_fee(&self, req: PermissionReq) -> ReturnType<EstimateFeeResp> {
    //     PermssionService::new()
    //         .await?
    //         .add_permission_fee(req)
    //         .await?
    //         .into()
    // }

    // // update permission
    // pub async fn up_permission(&self, req: PermissionReq, password: String) -> ReturnType<String> {
    //     PermssionService::new()
    //         .await?
    //         .up_permission(req, password)
    //         .await?
    //         .into()
    // }

    // // delegate permission
    // pub async fn del_permission(
    //     &self,
    //     grantor_addr: String,
    //     active_id: i8,
    //     password: String,
    // ) -> ReturnType<String> {
    //     PermssionService::new()
    //         .await?
    //         .del_permission(grantor_addr, active_id, password)
    //         .await?
    //         .into()
    // }
}
