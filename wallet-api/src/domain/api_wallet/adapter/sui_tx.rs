use crate::{
    error::service::ServiceError,
    domain::{
        api_wallet::adapter::{
            TIME_OUT,
            tx::{Multisig, Tx},
        },
        chain::TransferResp,
        coin::TokenCurrencyGetter,
    },
    infrastructure::swap_client::AggQuoteResp,
    request::{
        api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
        transaction::{ApproveReq, DepositReq, QuoteReq, SwapReq, WithdrawReq},
    },
    response_vo::{CommonFeeDetails, MultisigQueueFeeParams, TransferParams},
};
use alloy::primitives::U256;
use std::collections::HashMap;
use wallet_chain_interact::{
    Error, QueryTransactionResult,
    sui::{Provider, SuiChain, transfer::TransferOpt},
    tron::protocol::account::AccountResourceDetail,
    types::{ChainPrivateKey, FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp},
};
use wallet_database::entities::{
    api_assets::ApiAssetsEntity, coin::CoinEntity, multisig_account::MultisigAccountEntity,
    multisig_member::MultisigMemberEntities, multisig_queue::MultisigQueueEntity,
    permission::PermissionEntity,
};
use wallet_transport::client::RpcClient;
use wallet_transport_backend::api::BackendApi;
use wallet_utils::unit;

pub(crate) struct SuiTx {
    chain: SuiChain,
}

impl SuiTx {
    pub fn new(
        rpc_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, wallet_chain_interact::Error> {
        let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let rpc = RpcClient::new(rpc_url, header_opt, timeout)?;

        let provider = Provider::new(rpc);

        let sui_chain = SuiChain::new(provider)?;
        Ok(Self { chain: sui_chain })
    }
}

#[async_trait::async_trait]
impl Tx for SuiTx {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<AccountResourceDetail, ServiceError> {
        todo!()
    }

    async fn balance(&self, addr: &str, token: Option<String>) -> Result<U256, Error> {
        self.chain.balance(addr, token).await
    }

    async fn block_num(&self) -> Result<u64, Error> {
        self.chain.block_num().await
    }

    async fn query_tx_res(&self, hash: &str) -> Result<Option<QueryTransactionResult>, Error> {
        self.chain.query_tx_res(hash).await
    }

    async fn decimals(&self, token: &str) -> Result<u8, Error> {
        self.chain.decimals(token).await
    }

    async fn token_symbol(&self, token: &str) -> Result<String, Error> {
        self.chain.token_symbol(token).await
    }

    async fn token_name(&self, token: &str) -> Result<String, Error> {
        self.chain.token_name(token).await
    }

    async fn black_address(&self, token: &str, owner: &str) -> Result<bool, ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        let transfer_amount = self.check_min_transfer(&params.base.value, params.base.decimals)?;
        let balance =
            self.chain.balance(&params.base.from, params.base.token_address.clone()).await?;
        if balance < transfer_amount {
            return Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::InsufficientBalance))?;
        }

        tracing::info!("transfer ------------------- 9:");

        let req = TransferOpt::new(
            &params.base.from,
            &params.base.to,
            transfer_amount,
            params.base.token_address.clone(),
        )?;

        let mut helper = req.select_coin(&self.chain.provider).await?;
        tracing::info!("transfer ------------------- 10:");
        let pt = req.build_pt(&self.chain.provider, &mut helper, None).await?;

        tracing::info!("transfer ------------------- 11:");
        let gas = self.chain.estimate_fee(&params.base.from, pt).await?;

        let mut trans_fee = U256::from(gas.get_fee());
        if params.base.token_address.is_none() {
            trans_fee += transfer_amount;
            if balance < trans_fee {
                return Err(crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::InsufficientFeeBalance,
                ))?;
            }
        } else {
            let balance = self.chain.balance(&params.base.from, None).await?;
            if balance < trans_fee {
                return Err(crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::InsufficientFeeBalance,
                ))?;
            }
        }

        let fee = gas.get_fee_f64();
        let tx_data = req.build_data(&self.chain.provider, helper, gas).await?;
        let tx_hash = self.chain.exec(tx_data, private_key).await?;

        Ok(TransferResp::new(tx_hash, fee.to_string()))
    }

    async fn estimate_fee(
        &self,
        req: ApiBaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &req.chain_code, main_symbol, None).await?;

        let amount = unit::convert_to_u256(&req.value, req.decimals)?;
        let params = TransferOpt::new(&req.from, &req.to, amount, req.token_address.clone())?;

        let mut helper = params.select_coin(&self.chain.provider).await?;
        let pt = params.build_pt(&self.chain.provider, &mut helper, None).await?;

        let gas = self.chain.estimate_fee(&req.from, pt).await?;

        let res = CommonFeeDetails::new(gas.get_fee_f64(), token_currency, currency)?;

        let fee = wallet_utils::serde_func::serde_to_string(&res)?;
        Ok(fee)
    }

    async fn approve(
        &self,
        _req: &ApproveReq,
        _key: ChainPrivateKey,
        _value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn approve_fee(
        &self,
        _req: &ApproveReq,
        _value: U256,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn allowance(
        &self,
        _from: &str,
        _token: &str,
        _spender: &str,
    ) -> Result<U256, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn swap_quote(
        &self,
        _req: &QuoteReq,
        _quote_resp: &AggQuoteResp,
        _symbol: &str,
    ) -> Result<(U256, String, String), ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn swap(
        &self,
        _req: &SwapReq,
        _fee: String,
        _key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn deposit_fee(
        &self,
        _req: DepositReq,
        _main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn deposit(
        &self,
        _req: &DepositReq,
        _fee: String,
        _key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn withdraw_fee(
        &self,
        _req: WithdrawReq,
        _main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }

    async fn withdraw(
        &self,
        _req: &WithdrawReq,
        _fee: String,
        _key: ChainPrivateKey,
        _value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::NotSupportChain).into())
    }
}

#[async_trait::async_trait]
impl Multisig for SuiTx {
    async fn multisig_address(
        &self,
        _account: &MultisigAccountEntity,
        _member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, ServiceError> {
        todo!()
    }

    async fn deploy_multisig_account(
        &self,
        _account: &MultisigAccountEntity,
        _member: &MultisigMemberEntities,
        _fee_setting: Option<String>,
        _key: ChainPrivateKey,
    ) -> Result<(String, String), ServiceError> {
        todo!()
    }

    async fn deploy_multisig_fee(
        &self,
        _account: &MultisigAccountEntity,
        _member: MultisigMemberEntities,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn build_multisig_fee(
        &self,
        _req: &MultisigQueueFeeParams,
        _account: &MultisigAccountEntity,
        _decimal: u8,
        _token: Option<String>,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn build_multisig_with_account(
        &self,
        _req: &TransferParams,
        _account: &MultisigAccountEntity,
        _assets: &ApiAssetsEntity,
        _key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, ServiceError> {
        todo!()
    }

    async fn build_multisig_with_permission(
        &self,
        _req: &TransferParams,
        _p: &PermissionEntity,
        _coin: &CoinEntity,
    ) -> Result<MultisigTxResp, ServiceError> {
        todo!()
    }

    async fn sign_fee(
        &self,
        _account: &MultisigAccountEntity,
        _address: &str,
        _raw_data: &str,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn sign_multisig_tx(
        &self,
        _account: &MultisigAccountEntity,
        _address: &str,
        _key: ChainPrivateKey,
        _raw_data: &str,
    ) -> Result<MultisigSignResp, ServiceError> {
        todo!()
    }

    async fn estimate_multisig_fee(
        &self,
        _queue: &MultisigQueueEntity,
        _coin: &CoinEntity,
        _backend: &BackendApi,
        _sign_list: Vec<String>,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }
}
