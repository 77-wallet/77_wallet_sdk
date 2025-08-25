use crate::domain::api_wallet::adapter::sui_tx::SuiTx;
use crate::domain::api_wallet::adapter::{Multisig, TIME_OUT, Tx};
use crate::domain::chain::TransferResp;
use crate::infrastructure::swap_client::AggQuoteResp;
use crate::request::transaction::{
    ApproveReq, BaseTransferReq, DepositReq, QuoteReq, SwapReq, TransferReq, WithdrawReq,
};
use crate::response_vo::{MultisigQueueFeeParams, TransferParams};
use crate::{ServiceError, domain, response_vo};
use alloy::primitives::U256;
use std::collections::HashMap;
use wallet_chain_interact::types::{FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp};
use wallet_chain_interact::{
    Error, QueryTransactionResult,
    ton::{
        Cell,
        chain::TonChain,
        operations::{BuildInternalMsg, token_transfer::TokenTransferOpt, transfer::TransferOpt},
        provider::Provider,
    },
    types::ChainPrivateKey,
};
use wallet_database::entities::api_assets::ApiAssetsEntity;
use wallet_database::entities::multisig_account::MultisigAccountEntity;
use wallet_database::entities::multisig_member::MultisigMemberEntities;
use wallet_database::entities::multisig_queue::MultisigQueueEntity;
use wallet_database::entities::permission::PermissionEntity;
use wallet_database::{entities::coin::CoinEntity, repositories::api_account::ApiAccountRepo};
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
        let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let http_client = HttpClient::new(rpc_url, header_opt, timeout)?;
        let provider = Provider::new(http_client);

        let ton = TonChain::new(provider)?;
        Ok(Self { chain: ton })
    }

    pub async fn build_ext_cell(
        &self,
        req: &BaseTransferReq,
        provider: &Provider,
        address_type: TonAddressType,
    ) -> Result<Cell, crate::ServiceError> {
        if let Some(token) = req.token_address.clone() {
            let value = unit::convert_to_u256(&req.value, req.decimals)?;
            let arg = TokenTransferOpt::new(&req.from, &req.to, &token, value, req.spend_all)?;

            Ok(arg.build_trans(address_type, provider).await?)
        } else {
            let arg = TransferOpt::new(&req.from, &req.to, &req.value, req.spend_all)?;

            Ok(arg.build_trans(address_type, provider).await?)
        }
    }
}

#[async_trait::async_trait]
impl Tx for TonTx {
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
        params: &TransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        let transfer_amount = Self::check_min_transfer(&params.base.value, params.base.decimals)?;
        // 验证余额
        let balance = self
            .chain
            .balance(&params.base.from, params.base.token_address.clone())
            .await?;
        if balance < transfer_amount {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientBalance,
            ))?;
        }

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params.base.from,
            &params.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(crate::BusinessError::Account(
            crate::AccountError::NotFound(params.base.from.to_string()),
        ))?;

        let address_type = TonAddressType::try_from(account.address_type.as_str())?;

        let msg_cell = self
            .build_ext_cell(&params.base, &self.chain.provider, address_type)
            .await?;

        let fee = self
            .chain
            .estimate_fee(msg_cell.clone(), &params.base.from, address_type)
            .await?;

        let mut trans_fee = U256::from(fee.get_fee());
        if params.base.token_address.is_none() {
            // 主笔转账
            if !params.base.spend_all {
                trans_fee += transfer_amount;
                if balance < trans_fee {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientFeeBalance,
                    ))?;
                }
            }
        } else {
            let balance = self.chain.balance(&params.base.from, None).await?;
            if balance < trans_fee {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::InsufficientFeeBalance,
                ))?;
            }
        }
        let tx_hash = self.chain.exec(msg_cell, private_key, address_type).await?;

        Ok(TransferResp::new(tx_hash, fee.get_fee_ton().to_string()))
    }

    async fn estimate_fee(
        &self,
        req: BaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency = domain::coin::token_price::TokenCurrencyGetter::get_currency(
            currency,
            &req.chain_code,
            main_symbol,
            None,
        )
        .await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account =
            ApiAccountRepo::find_one_by_address_chain_code(&req.from, &req.chain_code, &pool)
                .await?
                .ok_or(crate::BusinessError::Account(
                    crate::AccountError::NotFound(req.from.to_string()),
                ))?;

        let address_type = TonAddressType::try_from(account.address_type.as_str())?;

        let msg_cell = self
            .build_ext_cell(&req, &self.chain.provider, address_type)
            .await?;

        let fee = self
            .chain
            .estimate_fee(msg_cell.clone(), &req.from, address_type)
            .await?;

        let res = response_vo::CommonFeeDetails::new(fee.get_fee_ton(), token_currency, currency)?;

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
impl Multisig for TonTx {
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

    async fn estimate_fee(
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
