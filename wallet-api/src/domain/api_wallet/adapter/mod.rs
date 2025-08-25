use alloy::primitives::U256;
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;
use wallet_chain_interact::types::{
    ChainPrivateKey, FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp,
};
use wallet_utils::unit;

use crate::{
    domain::chain::TransferResp,
    infrastructure::swap_client::AggQuoteResp,
    request::transaction::{
        ApproveReq, BaseTransferReq, DepositReq, QuoteReq, SwapReq, TransferReq, WithdrawReq,
    },
    response_vo::{MultisigQueueFeeParams, TransferParams},
};

use wallet_database::entities::{
    api_assets::ApiAssetsEntity, coin::CoinEntity, multisig_account::MultisigAccountEntity,
    multisig_member::MultisigMemberEntities, multisig_queue::MultisigQueueEntity,
    permission::PermissionEntity,
};
use wallet_transport_backend::{
    api::BackendApi,
    response_vo::chain::GasOracle,
};

pub mod btc_tx;
pub mod doge_tx;
pub mod eth_tx;
pub mod ltx_tx;
pub mod sol_tx;
pub mod sui_tx;
pub mod ton_tx;
pub mod tron_tx;
pub mod tx;

const TIME_OUT: u64 = 30;

#[async_trait::async_trait]
pub trait Oracle {
    async fn gas_oracle(&self) -> Result<GasOracle, crate::ServiceError>;

    async fn default_gas_oracle(&self) -> Result<GasOracle, crate::ServiceError>;
}

#[async_trait::async_trait]
pub trait Tx {
    fn check_min_transfer(
        &self,
        value: &str,
        decimal: u8,
    ) -> Result<U256, crate::ServiceError> {
        let min = U256::from(1);
        let transfer_amount = unit::convert_to_u256(value, decimal)?;

        if transfer_amount < min {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::AmountLessThanMin,
            ))?;
        }
        Ok(transfer_amount)
    }

    async fn account_resource(
        &self,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, crate::ServiceError>;

    async fn balance(
        &self,
        addr: &str,
        token: Option<String>,
    ) -> Result<U256, wallet_chain_interact::Error>;
    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error>;

    async fn query_tx_res(
        &self,
        hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>;

    async fn decimals(&self, token: &str) -> Result<u8, wallet_chain_interact::Error>;

    async fn token_symbol(&self, token: &str) -> Result<String, wallet_chain_interact::Error>;

    async fn token_name(&self, token: &str) -> Result<String, wallet_chain_interact::Error>;

    async fn black_address(&self, token: &str, owner: &str) -> Result<bool, crate::ServiceError>;

    async fn transfer(
        &self,
        params: &TransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, crate::ServiceError>;

    async fn estimate_fee(
        &self,
        req: BaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;

    async fn approve(
        &self,
        req: &ApproveReq,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, crate::ServiceError>;

    async fn approve_fee(
        &self,
        req: &ApproveReq,
        value: U256,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;

    async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<U256, crate::ServiceError>;

    async fn swap_quote(
        &self,
        req: &QuoteReq,
        quote_resp: &AggQuoteResp,
        symbol: &str,
    ) -> Result<(U256, String, String), crate::ServiceError>;

    async fn swap(
        &self,
        req: &SwapReq,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferResp, crate::ServiceError>;

    async fn deposit_fee(
        &self,
        req: DepositReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), crate::ServiceError>;

    async fn deposit(
        &self,
        req: &DepositReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, crate::ServiceError>;

    async fn withdraw_fee(
        &self,
        req: WithdrawReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), crate::ServiceError>;

    async fn withdraw(
        &self,
        req: &WithdrawReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, crate::ServiceError>;
}

#[async_trait::async_trait]
pub trait Multisig {
    async fn multisig_address(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, crate::ServiceError>;

    async fn deploy_multisig_account(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
        fee_setting: Option<String>,
        key: ChainPrivateKey,
    ) -> Result<(String, String), crate::ServiceError>;

    async fn deploy_multisig_fee(
        &self,
        account: &MultisigAccountEntity,
        member: MultisigMemberEntities,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;

    async fn build_multisig_fee(
        &self,
        req: &MultisigQueueFeeParams,
        account: &MultisigAccountEntity,
        decimal: u8,
        token: Option<String>,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;

    async fn build_multisig_with_account(
        &self,
        req: &TransferParams,
        account: &MultisigAccountEntity,
        assets: &ApiAssetsEntity,
        key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, crate::ServiceError>;

    async fn build_multisig_with_permission(
        &self,
        req: &TransferParams,
        p: &PermissionEntity,
        coin: &CoinEntity,
    ) -> Result<MultisigTxResp, crate::ServiceError>;

    async fn sign_fee(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        raw_data: &str,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;

    async fn sign_multisig_tx(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        key: ChainPrivateKey,
        raw_data: &str,
    ) -> Result<MultisigSignResp, crate::ServiceError>;

    async fn estimate_fee(
        &self,
        queue: &MultisigQueueEntity,
        coin: &CoinEntity,
        backend: &BackendApi,
        sign_list: Vec<String>,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError>;
}
