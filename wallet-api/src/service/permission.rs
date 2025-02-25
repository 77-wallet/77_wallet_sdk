use crate::{
    domain::chain::adapter::ChainAdapterFactory,
    response_vo::permssion::{AccountPermission, PermissionList, PermissionResp},
};
use wallet_chain_interact::tron::TronChain;

pub struct PermssionService {
    chain: TronChain,
}

impl PermssionService {
    pub async fn new() -> Result<Self, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        Ok(Self { chain })
    }
}

impl PermssionService {
    // all permission category
    pub fn permission_list() -> Result<PermissionList, crate::ServiceError> {
        Ok(PermissionList::default())
    }
    // trans permssion
    pub fn permssion_trans() -> Result<Vec<i8>, crate::ServiceError> {
        Ok(PermissionList::trans_permission())
    }

    // account permisson
    pub async fn account_permssion(
        &self,
        address: String,
    ) -> Result<AccountPermission, crate::ServiceError> {
        let account = self.chain.account_info(&address).await?;

        let actives = account
            .active_permission
            .iter()
            .map(|p| PermissionResp::try_from(p).unwrap())
            .collect();

        Ok(AccountPermission {
            owner: PermissionResp::try_from(&account.owner_permission)?,
            actives,
        })
    }
}
