use wallet_chain_interact::{
    tron::{operations::stake, params::ResourceConsumer, TronChain},
    types::MultisigTxResp,
};

pub enum StakeArgs {
    Freeze(stake::FreezeBalanceArgs),
    UnFreeze(stake::UnFreezeBalanceArgs),
    CancelAllUnFreeze(stake::CancelAllFreezeBalanceArgs),
    Withdraw(stake::WithdrawBalanceArgs),
    Delegate(stake::DelegateArgs),
    UnDelegate(stake::UnDelegateArgs),
    Votes(stake::VoteWitnessArgs),
}

impl StakeArgs {
    pub async fn exec(
        self,
        account: &str,
        chain: &TronChain,
    ) -> Result<ResourceConsumer, crate::ServiceError> {
        let signature_num = 1;
        let res = match self {
            Self::Freeze(params) => chain.simple_fee(account, signature_num, params).await?,
            Self::UnFreeze(params) => chain.simple_fee(account, signature_num, params).await?,
            Self::CancelAllUnFreeze(params) => {
                chain.simple_fee(account, signature_num, params).await?
            }
            Self::Withdraw(params) => chain.simple_fee(account, signature_num, params).await?,
            Self::Delegate(params) => chain.simple_fee(account, signature_num, params).await?,
            Self::UnDelegate(params) => chain.simple_fee(account, signature_num, params).await?,
            Self::Votes(params) => chain.simple_fee(account, signature_num, params).await?,
        };
        Ok(res)
    }

    // 构建多签交易
    pub async fn build_multisig_tx(
        self,
        chain: &TronChain,
        expiration: u64,
    ) -> Result<MultisigTxResp, crate::ServiceError> {
        let res = match self {
            Self::Freeze(params) => chain.build_multisig_transaction(params, expiration).await?,
            Self::UnFreeze(params) => chain.build_multisig_transaction(params, expiration).await?,
            Self::CancelAllUnFreeze(params) => {
                chain.build_multisig_transaction(params, expiration).await?
            }
            Self::Withdraw(params) => chain.build_multisig_transaction(params, expiration).await?,
            Self::Delegate(params) => chain.build_multisig_transaction(params, expiration).await?,
            Self::UnDelegate(params) => {
                chain.build_multisig_transaction(params, expiration).await?
            }
            Self::Votes(params) => chain.build_multisig_transaction(params, expiration).await?,
        };
        Ok(res)
    }
}
