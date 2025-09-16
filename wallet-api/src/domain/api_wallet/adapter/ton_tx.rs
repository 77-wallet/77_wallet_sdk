use crate::{
    error::ServiceError,
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
    ton::{
        Cell,
        chain::TonChain,
        operations::{BuildInternalMsg, token_transfer::TokenTransferOpt, transfer::TransferOpt},
        provider::Provider,
    },
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
use wallet_transport::client::HttpClient;
use wallet_transport_backend::api::BackendApi;
use wallet_types::chain::address::r#type::TonAddressType;
use wallet_utils::unit;

pub(crate) struct TonTx {
    chain: TonChain,
}

impl TonTx {
    pub fn new(
        rpc_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, wallet_chain_interact::Error> {
        // let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let http_client = HttpClient::new(rpc_url, header_opt, timeout)?;
        let provider = Provider::new(http_client);

        let ton = TonChain::new(provider)?;
        Ok(Self { chain: ton })
    }

    pub async fn build_ext_cell(
        &self,
        req: &ApiBaseTransferReq,
        provider: &Provider,
        address_type: TonAddressType,
    ) -> Result<Cell, crate::error::service::ServiceError> {
        if let Some(token) = req.token_address.clone() {
            let value = unit::convert_to_u256(&req.value, req.decimals)?;
            let arg = TokenTransferOpt::new(&req.from, &req.to, &token, value, req.spend_all)?;

            Ok(arg.build_trans(address_type, provider).await?)
        } else {
            tracing::info!("transfer ------------------- 16:");
            let arg = TransferOpt::new(&req.from, &req.to, &req.value, req.spend_all)?;

            Ok(arg.build_trans(address_type, provider).await?)
        }
    }
}

#[async_trait::async_trait]
impl Tx for TonTx {
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
        tracing::info!("transfer ------------------- 11:");
        // 验证余额
        let balance =
            self.chain.balance(&params.base.from, params.base.token_address.clone()).await?;
        if balance < transfer_amount {
            return Err(crate::error::business::BusinessError::Chain(crate::error::business::chain::ChainError::InsufficientBalance))?;
        }
        tracing::info!("transfer ------------------- 12:");

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params.base.from,
            &params.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(crate::error::business::BusinessError::Account(crate::error::business::account::AccountError::NotFound(
            params.base.from.to_string(),
        )))?;

        let address_type = TonAddressType::try_from(account.address_type.as_str())?;

        tracing::info!("transfer ------------------- 13:");
        let msg_cell =
            self.build_ext_cell(&params.base, &self.chain.provider, address_type).await?;

        tracing::info!("transfer ------------------- 14:");
        let fee =
            self.chain.estimate_fee(msg_cell.clone(), &params.base.from, address_type).await?;

        let mut trans_fee = U256::from(fee.get_fee());
        if params.base.token_address.is_none() {
            // 主笔转账
            if !params.base.spend_all {
                trans_fee += transfer_amount;
                if balance < trans_fee {
                    return Err(crate::error::business::BusinessError::Chain(
                        crate::error::business::chain::ChainError::InsufficientFeeBalance,
                    ))?;
                }
            }
        } else {
            let balance = self.chain.balance(&params.base.from, None).await?;
            if balance < trans_fee {
                return Err(crate::error::business::BusinessError::Chain(
                    crate::error::business::chain::ChainError::InsufficientFeeBalance,
                ))?;
            }
        }
        tracing::info!("transfer ------------------- 15:");
        let tx_hash = self.chain.exec(msg_cell, private_key, address_type).await?;

        Ok(TransferResp::new(tx_hash, fee.get_fee_ton().to_string()))
    }

    async fn estimate_fee(
        &self,
        req: ApiBaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        // let backend = crate::manager::Context::get_global_backend_api()?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &req.chain_code, main_symbol, None).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let account =
            ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
                .await?
                .ok_or(crate::error::business::BusinessError::Account(crate::error::business::account::AccountError::NotFound(
                    req.from.to_string(),
                )))?;

        let address_type = TonAddressType::try_from(account.address_type.as_str())?;

        let msg_cell = self.build_ext_cell(&req, &self.chain.provider, address_type).await?;

        let fee = self.chain.estimate_fee(msg_cell.clone(), &req.from, address_type).await?;

        let res = CommonFeeDetails::new(fee.get_fee_ton(), token_currency, currency)?;

        let res = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok(res)
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
        _value: U256,
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
impl Multisig for TonTx {
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
