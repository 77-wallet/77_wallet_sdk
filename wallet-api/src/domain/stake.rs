use wallet_chain_interact::{
    tron::{
        operations::{stake, TronSimulateOperation},
        params::ResourceConsumer,
        TronChain,
    },
    types::MultisigTxResp,
};

pub enum StakeArgs {
    // 质押
    Freeze(stake::FreezeBalanceArgs),
    // 解冻
    UnFreeze(stake::UnFreezeBalanceArgs),
    // 取消解锁
    CancelAllUnFreeze(stake::CancelAllFreezeBalanceArgs),
    // 提取可以
    Withdraw(stake::WithdrawBalanceArgs),
    // 代理
    Delegate(stake::DelegateArgs),
    // 批量代理
    BatchDelegate(Vec<stake::DelegateArgs>),
    // 取消代理
    UnDelegate(stake::UnDelegateArgs),
    // 批量取消代理
    BatchUnDelegate(Vec<stake::UnDelegateArgs>),
    // 投票
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
            Self::Delegate(params) => {
                chain
                    .simulate_simple_fee(account, "", signature_num, params)
                    .await?
            }
            Self::UnDelegate(params) => {
                chain
                    .simulate_simple_fee(account, "", signature_num, params)
                    .await?
            }
            Self::BatchDelegate(mut params) => {
                let item = params.remove(0);

                let mut consumer = chain
                    .simulate_simple_fee(account, "", signature_num, item)
                    .await?;

                for item in params {
                    let raw_data_hex = item.simulate_raw_transaction()?;

                    let size = chain.provider.calc_bandwidth(&raw_data_hex, signature_num);
                    consumer.bandwidth.consumer += size;
                }
                consumer
            }
            Self::BatchUnDelegate(mut params) => {
                let item = params.remove(0);

                let mut consumer = chain
                    .simulate_simple_fee(account, "", signature_num, item)
                    .await?;

                for item in params {
                    let raw_data_hex = item.simulate_raw_transaction()?;

                    let size = chain.provider.calc_bandwidth(&raw_data_hex, signature_num);
                    consumer.bandwidth.consumer += size;
                }
                consumer
            }

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
            _ => {
                return Err(crate::BusinessError::Stake(
                    crate::StakeError::MultisigUnSupportBillKind,
                ))?
            }
        };
        Ok(res)
    }
}
