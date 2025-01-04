use super::ReturnType;
use crate::{
    request::stake::{
        BatchDelegate, BatchUnDelegate, CancelAllUnFreezeReq, DelegateReq, FreezeBalanceReq,
        UnDelegateReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq,
    },
    response_vo::{
        self,
        account::AccountResource,
        stake::{
            AddressExists, BatchDelegateResp, CancelAllUnFreezeResp, DelegateListResp,
            DelegateResp, FreezeListResp, FreezeResp, ResourceResp, UnfreezeListResp,
            WithdrawUnfreezeResp,
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

    pub async fn address_exists(&self, accounts: Vec<String>) -> ReturnType<Vec<AddressExists>> {
        StackService::new()
            .await?
            .account_exists(accounts)
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

    pub async fn batch_delegate(
        &self,
        req: BatchDelegate,
        password: String,
    ) -> ReturnType<BatchDelegateResp> {
        StackService::new()
            .await?
            .batch_delegate(req, password)
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

    pub async fn batch_un_deleate(
        &self,
        req: BatchUnDelegate,
        password: String,
    ) -> ReturnType<BatchDelegateResp> {
        StackService::new()
            .await?
            .batch_un_delegate(req, password)
            .await?
            .into()
    }

    pub async fn delegate_to_other(
        &self,
        owner_address: String,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Vec<DelegateListResp>> {
        StackService::new()
            .await?
            .delegate_to_other(&owner_address, page, page_size)
            .await?
            .into()
    }

    pub async fn delegate_from_other(
        &self,
        owner_address: String,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Vec<DelegateListResp>> {
        StackService::new()
            .await?
            .delegate_from_other(&owner_address, page, page_size)
            .await?
            .into()
    }

    // ************************************************ vote *********************************************************
    pub async fn votes(&self, req: VoteWitnessReq, password: &str) -> ReturnType<String> {
        StackService::new()
            .await?
            .votes(req, password)
            .await?
            .into()
    }

    pub async fn voter_info(&self, owner: &str) -> ReturnType<response_vo::stake::VoterInfoResp> {
        StackService::new().await?.voter_info(owner).await?.into()
    }

    pub async fn votes_node_list(
        &self,
        owner_address: Option<&str>,
    ) -> ReturnType<response_vo::stake::VoteListResp> {
        StackService::new()
            .await?
            .vote_list(owner_address)
            .await?
            .into()
    }

    pub async fn top_witness(&self) -> ReturnType<Option<response_vo::stake::Witness>> {
        StackService::new().await?.top_witness().await?.into()
    }

    pub async fn claim_votes_rewards(
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
    use crate::{
        request::stake::{VoteWitnessReq, VotesReq},
        test::env::{setup_test_environment, TestData},
    };

    #[tokio::test]
    async fn test_votes() {
        wallet_utils::init_test_log();
        let TestData { wallet_manager, .. } = setup_test_environment(None, None, false, None)
            .await
            .unwrap();

        let owner_address = "TFdDqaoMkPbWWv9EUTbmfGP142f9ysiJq2";
        let vote_witness_req = VoteWitnessReq::new(
            owner_address,
            vec![VotesReq::new("TA4pHhHgobzSGH3CWPsZ5URNk3QkzUEggX", 1)],
        ); // You may need to import this struct
        let password = "123456"; // Replace with the actual password
        tracing::warn!(
            "{:#?}",
            wallet_utils::serde_func::serde_to_string(&vote_witness_req)
        );
        let res = wallet_manager.votes(vote_witness_req, password).await;
        println!("{:#?}", res);
        let res = wallet_utils::serde_func::serde_to_string(&res);
        tracing::info!("{:#?}", res);
    }
    #[tokio::test]
    async fn test_votes_node_list() {
        wallet_utils::init_test_log();
        let TestData { wallet_manager, .. } = setup_test_environment(None, None, false, None)
            .await
            .unwrap();

        let owner_address = Some("TC5LVLjiMXkPhmNDJaHf4N2nuXr4V6foaZ");
        let res = wallet_manager.votes_node_list(owner_address).await;
        println!("{:#?}", res);
        let res = wallet_utils::serde_func::serde_to_string(&res);
        tracing::info!("{:#?}", res);
    }

    #[tokio::test]
    async fn test_votes_top_rewards() {
        wallet_utils::init_test_log();
        let TestData { wallet_manager, .. } = setup_test_environment(None, None, false, None)
            .await
            .unwrap();

        let res = wallet_manager.top_witness().await;
        println!("{:#?}", res);
        let res = wallet_utils::serde_func::serde_to_string(&res);
        tracing::info!("{:#?}", res);
    }

    #[tokio::test]
    async fn test_voter_info() {
        wallet_utils::init_test_log();
        let TestData { wallet_manager, .. } = setup_test_environment(None, None, false, None)
            .await
            .unwrap();

        let owner = "TC5LVLjiMXkPhmNDJaHf4N2nuXr4V6foaZ";
        let res = wallet_manager.voter_info(owner).await;
        tracing::info!("{:#?}", res);
        let res = wallet_utils::serde_func::serde_to_string(&res);
        tracing::info!("{:#?}", res);
    }
}

#[cfg(test)]
mod cal_tests {

    // Function to calculate the voter reward
    fn calculate_voter_reward(
        total_reward: f64,
        voter_votes: f64,
        sr_votes: f64,
        total_sr_votes: f64,
        voter_share: f64,
    ) -> f64 {
        total_reward * (sr_votes / total_sr_votes) * voter_share * (voter_votes / sr_votes)
    }

    fn calculate_block_reward(voter_share: f64, voter_votes: f64, sr_votes: f64) -> f64 {
        460800.0 / 27.0 * voter_share * voter_votes / sr_votes
    }

    // Function to calculate APR
    fn calculate_apr(voter_reward: f64, block_reward: f64, voter_votes: f64) -> f64 {
        if voter_votes == 0.0 {
            return 0.0;
        }
        ((voter_reward + block_reward) / voter_votes) * 100.0 * 365.0
    }

    #[test]
    fn test_calculate_apr() {
        // Parameters for the test case
        let total_reward = 4_608_000.0; // Total reward pool

        let total_sr_votes = 39797663721.0; // Total votes of all SR and SRP

        // let sr_votes = 3069539068.0; // Votes obtained by the SR
        let sr_votes = 1248080337.0; // Votes obtained by the SR

        let voter_votes = 10_000_000.0; // Voter's votes
        let voter_share = 1.0; // Voter share (80%)
                               // let voter_share = 0.90; // Voter share (80%)

        // Calculate voter reward
        let voter_reward = calculate_voter_reward(
            total_reward,
            voter_votes,
            sr_votes,
            total_sr_votes,
            voter_share,
        );

        let block_reward = calculate_block_reward(voter_share, voter_votes, sr_votes);
        println!("block reward: {}", block_reward);
        // Calculate APR
        let apr = calculate_apr(voter_reward, block_reward, voter_votes);

        // Debug output
        println!("Voter Reward: {:.2}", voter_reward);
        println!("Voter APR: {:.2}", apr);

        // Assert results (expected values based on the example)
        assert!((voter_reward - 1272.10).abs() < 1e-2); // Reward should be close to 1272.10 TRX
        assert!((apr - 12.72).abs() < 1e-2); // APR should be close to 12.72%
    }
}
