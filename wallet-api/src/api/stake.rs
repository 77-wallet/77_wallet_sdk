use super::ReturnType;
use crate::{
    request::stake::{
        CancelAllUnFreezeReq, DelegateReq, FreezeBalanceReq, UnDelegateReq, UnFreezeBalanceReq,
        VoteWitnessReq, WithdrawBalanceReq,
    },
    response_vo::{
        self,
        account::AccountResource,
        stake::{
            CancelAllUnFreezeResp, DelegateListResp, DelegateResp, FreezeListResp, FreezeResp,
            ResourceResp, ResourceToTrxResp, TrxToResourceResp, UnfreezeListResp,
            WithdrawUnfreezeResp,
        },
    },
    service::stake::StackService,
};
use wallet_transport_backend::response_vo::stake::{SystemEnergyResp, VoteListResp};

impl crate::WalletManager {
    // account resource
    pub async fn resource_info(&self, account: String) -> ReturnType<AccountResource> {
        let service = StackService::new().await?;
        service.account_resource(&account).await?.into()
    }

    pub async fn estimate_stake_fee(
        &self,
        bill_kind: i64,
        content: String,
    ) -> ReturnType<response_vo::EstimateFeeResp> {
        StackService::new()
            .await?
            .estimate_stake_fee(bill_kind, content)
            .await?
            .into()
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
        req: CancelAllUnFreezeReq,
        password: String,
    ) -> ReturnType<CancelAllUnFreezeResp> {
        StackService::new()
            .await?
            .cancel_all_unfreeze(req, password)
            .await
            .into()
    }

    /// Withdraws any unfrozen balances for the given owner address.
    pub async fn withdraw_unfreeze(
        &self,
        req: WithdrawBalanceReq,
        password: String,
    ) -> ReturnType<WithdrawUnfreezeResp> {
        StackService::new()
            .await?
            .withdraw_unfreeze(req, password)
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

    pub async fn trx_to_resource(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> ReturnType<TrxToResourceResp> {
        StackService::new()
            .await?
            .trx_to_resource(account, value, resource_type)
            .await?
            .into()
    }

    pub async fn resource_to_trx(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> ReturnType<ResourceToTrxResp> {
        StackService::new()
            .await?
            .resource_to_trx(account, value, resource_type)
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
    ) -> ReturnType<ResourceResp> {
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

    // ************************************************ vote *********************************************************
    // pub async fn votes_fee_estimation(&self, account: String) -> ReturnType<VoteFeeResp> {
    //     StackService::new()
    //         .await?
    //         .votes_fee_estimation(account)
    //         .await?
    //         .into()
    // }

    pub async fn votes(&self, req: VoteWitnessReq, password: &str) -> ReturnType<String> {
        StackService::new()
            .await?
            .votes(req, password)
            .await?
            .into()
    }

    pub async fn votes_overview(&self, account: String)
    // -> ReturnType<VoteOverviewResp>
    {
        todo!()
        // StackService::new()
        //     .await?
        //     .vote_overview(account)
        //     .await?
        //     .into()
    }

    pub async fn votes_node_list(
        &self,
    ) -> ReturnType<wallet_chain_interact::tron::operations::stake::vote_list::VoteWitnessResp>
    {
        StackService::new().await?.vote_list().await?.into()
    }

    pub async fn votes_top_rewards(&self, account: String)
    // -> ReturnType<VoteTopRewardsResp>
    {
        todo!()
        // StackService::new()
        //     .await?
        //     .vote_top_rewards(account)
        //     .await?
        //     .into()
    }

    pub async fn votes_claim_rewards(
        &self,
        req: WithdrawBalanceReq,
        password: &str,
    ) -> ReturnType<String> {
        StackService::new()
            .await?
            .votes_claim_rewards(req, password)
            .await?
            .into()
    }

    // ************************************************ multisig  *********************************************************
    pub async fn build_multisig_stake(
        &self,
        bill_kind: i64,
        content: String,
        expiration: i64,
        password: String,
    ) -> ReturnType<String> {
        StackService::new()
            .await?
            .build_multisig_stake(bill_kind, content, expiration, password)
            .await?
            .into()
    }
}

#[cfg(test)]
mod tests {
    use crate::test::env::{setup_test_environment, TestData};

    #[tokio::test]
    async fn test_votes_node_list() {
        wallet_utils::init_test_log();
        let TestData { wallet_manager, .. } = setup_test_environment(None, None, false, None)
            .await
            .unwrap();

        let phrase = wallet_manager.votes_node_list().await;
        println!("{:#?}", phrase);
    }
}
