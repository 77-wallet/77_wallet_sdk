use super::ReturnType;
use crate::{
    request::stake::{DelegateReq, FreezeBalanceReq, UnDelegateReq, UnFreezeBalanceReq},
    response_vo::{
        account::AccountResource,
        stake::{
            CanDelegatedResp, DelegateListResp, DelegateResp, EstimatedResourcesResp,
            FreezeListResp, FreezeResp, UnfreezeListResp, WithdrawUnfreezeResp,
        },
    },
    service::stake::StackService,
};
use wallet_transport_backend::response_vo::stake::SystemEnergyResp;

impl crate::WalletManager {
    // account resource
    pub async fn resource_info(&self, account: String) -> ReturnType<AccountResource> {
        let service = StackService::new().await?;
        service.account_resource(&account).await?.into()
    }

    // freeze balance
    pub async fn freeze_balance(
        &self,
        req: FreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        StackService::new()
            .await?
            .freeze_balance(req, &password)
            .await?
            .into()
    }

    // un freeze balance
    pub async fn un_freeze_balance(
        &self,
        req: UnFreezeBalanceReq,
        password: String,
    ) -> ReturnType<FreezeResp> {
        StackService::new()
            .await?
            .un_freeze_balance(req, &password)
            .await?
            .into()
    }

    // freeze list
    pub async fn freeze_list(&self, owner_address: String) -> ReturnType<Vec<FreezeListResp>> {
        StackService::new()
            .await?
            .freeze_list(&owner_address)
            .await?
            .into()
    }

    pub async fn un_freeze_list(&self, owner_address: String) -> ReturnType<Vec<UnfreezeListResp>> {
        StackService::new()
            .await?
            .un_freeze_list(&owner_address)
            .await?
            .into()
    }

    pub async fn cancel_all_unfreeze(
        &self,
        owner_address: String,
        password: String,
    ) -> ReturnType<String> {
        StackService::new()
            .await?
            .cancel_all_unfreeze(&owner_address, &password)
            .await
            .into()
    }

    /// Withdraws any unfrozen balances for the given owner address.
    pub async fn withdraw_unfreeze(
        &self,
        owner_address: String,
        password: String,
    ) -> ReturnType<WithdrawUnfreezeResp> {
        StackService::new()
            .await?
            .withdraw_unfreeze(&owner_address, &password)
            .await?
            .into()
    }

    pub async fn request_resource(
        &self,
        _account: String,
        _energy: i64,
        _bandwidth: i64,
        _value: String,
        _symbol: String,
        _to: String,
    ) -> ReturnType<String> {
        // let repo = self.repo_factory.stake_repo();
        // StackService::new(repo)
        //     .request_resource(account, energy, bandwidth, &value, &symbol, &to)
        //     .await?
        //     .into()
        "used request_energy to instead".to_string().into()
    }

    pub async fn get_estimated_resources(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> ReturnType<EstimatedResourcesResp> {
        StackService::new()
            .await?
            .get_estimated_resources(account, value, resource_type)
            .await?
            .into()
    }

    pub async fn system_resource(&self, account: String) -> ReturnType<SystemEnergyResp> {
        StackService::new()
            .await?
            .system_resource(account)
            .await?
            .into()
    }

    pub async fn request_energy(&self, account: String, energy: i64) -> ReturnType<String> {
        StackService::new()
            .await?
            .request_energy(account, energy)
            .await?
            .into()
    }

    // ************************************************ delegate *********************************************************
    pub async fn get_can_delegated_max(
        &self,
        account: String,
        resource_type: String,
    ) -> ReturnType<CanDelegatedResp> {
        StackService::new()
            .await?
            .can_delegated_max(account, resource_type)
            .await?
            .into()
    }

    pub async fn delegate_resource(
        &self,
        req: DelegateReq,
        password: String,
    ) -> ReturnType<DelegateResp> {
        StackService::new()
            .await?
            .delegate_resource(req, &password)
            .await?
            .into()
    }

    // 回收资源
    pub async fn un_delegate_resource(
        &self,
        req: UnDelegateReq,
        password: String,
    ) -> ReturnType<DelegateResp> {
        StackService::new()
            .await?
            .un_delegate_resource(req, password)
            .await?
            .into()
    }

    pub async fn delegate_to_other(
        &self,
        owner_address: String,
    ) -> ReturnType<Vec<DelegateListResp>> {
        StackService::new()
            .await?
            .delegate_to_other(&owner_address)
            .await?
            .into()
    }

    pub async fn delegate_from_other(
        &self,
        owner_address: String,
    ) -> ReturnType<Vec<DelegateListResp>> {
        StackService::new()
            .await?
            .delegate_from_other(&owner_address)
            .await?
            .into()
    }
}
