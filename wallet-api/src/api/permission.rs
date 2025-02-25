use super::ReturnType;
use crate::{
    response_vo::permssion::{AccountPermission, PermissionList},
    service::permission::PermssionService,
};

impl crate::WalletManager {
    pub fn permission_list(&self) -> ReturnType<PermissionList> {
        PermssionService::permission_list().into()
    }

    pub fn permssion_trans(&self) -> ReturnType<Vec<i8>> {
        PermssionService::permssion_trans().into()
    }

    pub async fn account_permission(&self, address: String) -> ReturnType<AccountPermission> {
        PermssionService::new()
            .await?
            .account_permssion(address)
            .await?
            .into()
    }
}
