use crate::{
    ServiceError,
    domain::{
        api_wallet::adapter::{Multisig, TIME_OUT, Tx},
        chain::TransferResp,
        coin::TokenCurrencyGetter,
    },
    infrastructure::swap_client::AggQuoteResp,
    request::{
        api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
        transaction::{
            ApproveReq, BaseTransferReq, DepositReq, QuoteReq, SwapReq, TransferReq, WithdrawReq,
        },
    },
    response_vo::{CommonFeeDetails, MultisigQueueFeeParams, TransferParams},
};
use alloy::primitives::U256;
use std::collections::HashMap;
use wallet_chain_interact::{
    Error, QueryTransactionResult,
    ltc::{LtcChain, operations::transfer::TransferArg, provider::ProviderConfig},
    tron::protocol::account::AccountResourceDetail,
    types::{ChainPrivateKey, FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp},
};
use wallet_database::{
    entities::{
        api_assets::ApiAssetsEntity, coin::CoinEntity, multisig_account::MultisigAccountEntity,
        multisig_member::MultisigMemberEntities, multisig_queue::MultisigQueueEntity,
        permission::PermissionEntity,
    },
    repositories::api_account::ApiAccountRepo,
};
use wallet_transport_backend::api::BackendApi;
use wallet_types::chain::address::r#type::LtcAddressType;

pub(crate) struct LtcTx {
    chin: LtcChain,
}

impl LtcTx {
    pub fn new(rpc_url: &str, header_opt: Option<HashMap<String, String>>) -> Result<Self, Error> {
        let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let config = ProviderConfig {
            rpc_url: rpc_url.to_string(),
            rpc_auth: None,
            http_url: rpc_url.to_string(),
            http_api_key: None,
            access_key: None,
        };
        let ltc_chain = LtcChain::new(config, network, header_opt, timeout)?;
        Ok(Self { chin: ltc_chain })
    }

    pub fn handle_ltc_fee_error(&self, err: wallet_chain_interact::Error) -> crate::ServiceError {
        match err {
            Error::UtxoError(wallet_chain_interact::UtxoError::InsufficientBalance) => {
                crate::BusinessError::Chain(crate::ChainError::InsufficientBalance).into()
            }
            Error::UtxoError(wallet_chain_interact::UtxoError::InsufficientFee(_fee)) => {
                crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance).into()
            }
            Error::UtxoError(wallet_chain_interact::UtxoError::ExceedsMaximum) => {
                crate::BusinessError::Chain(crate::ChainError::ExceedsMaximum).into()
            }
            Error::UtxoError(wallet_chain_interact::UtxoError::DustTx) => {
                crate::BusinessError::Chain(crate::ChainError::DustTransaction).into()
            }
            Error::UtxoError(wallet_chain_interact::UtxoError::ExceedsMaxFeeRate) => {
                crate::BusinessError::Chain(crate::ChainError::ExceedsMaxFeerate).into()
            }
            _ => err.into(),
        }
    }
}

#[async_trait::async_trait]
impl Tx for LtcTx {
    async fn account_resource(
        &self,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, ServiceError> {
        todo!()
    }

    async fn balance(&self, addr: &str, token: Option<String>) -> Result<U256, Error> {
        self.chin.balance(addr, token).await
    }

    async fn block_num(&self) -> Result<u64, Error> {
        self.chin.block_num().await
    }

    async fn query_tx_res(&self, hash: &str) -> Result<Option<QueryTransactionResult>, Error> {
        self.chin.query_tx_res(hash).await
    }

    async fn decimals(&self, token: &str) -> Result<u8, Error> {
        self.chin.decimals(token).await
    }

    async fn token_symbol(&self, token: &str) -> Result<String, Error> {
        self.chin.token_symbol(token).await
    }

    async fn token_name(&self, token: &str) -> Result<String, Error> {
        self.chin.token_name(token).await
    }

    async fn black_address(&self, token: &str, owner: &str) -> Result<bool, ServiceError> {
        Ok(false)
    }

    async fn transfer(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params.base.from,
            &params.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(crate::BusinessError::Account(crate::AccountError::NotFound(
            params.base.from.to_string(),
        )))?;

        let address_type = LtcAddressType::try_from(Some(account.address_type))?;

        let params = TransferArg::new(
            &params.base.from,
            &params.base.to,
            &params.base.value,
            address_type,
            self.chin.network,
        )?
        .with_spend_all(params.base.spend_all);

        let tx = self
            .chin
            .transfer(params, private_key)
            .await
            .map_err(|e| self.handle_ltc_fee_error(e))?;

        Ok(TransferResp::new(tx.tx_hash, tx.fee.to_string()))
    }

    async fn estimate_fee(
        &self,
        req: ApiBaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &req.chain_code, main_symbol, None).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account =
            ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
                .await?
                .ok_or(crate::BusinessError::Account(crate::AccountError::NotFound(
                    req.from.to_string(),
                )))?;

        let address_type = LtcAddressType::try_from(Some(account.address_type))?;

        let params =
            TransferArg::new(&req.from, &req.to, &req.value, address_type, self.chin.network)?
                .with_spend_all(req.spend_all);

        let fee = self.chin.estimate_fee(params).await.map_err(|e| self.handle_ltc_fee_error(e))?;

        let res = CommonFeeDetails::new(fee.transaction_fee_f64(), token_currency, currency)?;
        let res = wallet_utils::serde_func::serde_to_string(&res)?;
        Ok(res)
    }

    async fn approve(
        &self,
        req: &ApproveReq,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn approve_fee(
        &self,
        req: &ApproveReq,
        value: U256,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<U256, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn swap_quote(
        &self,
        req: &QuoteReq,
        quote_resp: &AggQuoteResp,
        symbol: &str,
    ) -> Result<(U256, String, String), ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn swap(
        &self,
        req: &SwapReq,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn deposit_fee(
        &self,
        req: DepositReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn deposit(
        &self,
        req: &DepositReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn withdraw_fee(
        &self,
        req: WithdrawReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }

    async fn withdraw(
        &self,
        req: &WithdrawReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        Err(crate::BusinessError::Chain(crate::ChainError::NotSupportChain).into())
    }
}

#[async_trait::async_trait]
impl Multisig for LtcTx {
    async fn multisig_address(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, ServiceError> {
        todo!()
    }

    async fn deploy_multisig_account(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
        fee_setting: Option<String>,
        key: ChainPrivateKey,
    ) -> Result<(String, String), ServiceError> {
        todo!()
    }

    async fn deploy_multisig_fee(
        &self,
        account: &MultisigAccountEntity,
        member: MultisigMemberEntities,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn build_multisig_fee(
        &self,
        req: &MultisigQueueFeeParams,
        account: &MultisigAccountEntity,
        decimal: u8,
        token: Option<String>,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn build_multisig_with_account(
        &self,
        req: &TransferParams,
        account: &MultisigAccountEntity,
        assets: &ApiAssetsEntity,
        key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, ServiceError> {
        todo!()
    }

    async fn build_multisig_with_permission(
        &self,
        req: &TransferParams,
        p: &PermissionEntity,
        coin: &CoinEntity,
    ) -> Result<MultisigTxResp, ServiceError> {
        todo!()
    }

    async fn sign_fee(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        raw_data: &str,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }

    async fn sign_multisig_tx(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        key: ChainPrivateKey,
        raw_data: &str,
    ) -> Result<MultisigSignResp, ServiceError> {
        todo!()
    }

    async fn estimate_multisig_fee(
        &self,
        queue: &MultisigQueueEntity,
        coin: &CoinEntity,
        backend: &BackendApi,
        sign_list: Vec<String>,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        todo!()
    }
}
