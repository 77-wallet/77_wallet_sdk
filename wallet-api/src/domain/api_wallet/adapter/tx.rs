use crate::error::{service::ServiceError /*, system::SystemError*/}; // SystemError未使用
use alloy::primitives::U256;
use wallet_chain_interact::{
    tron::protocol::account::AccountResourceDetail,
    types::{FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp},
};
use wallet_utils::unit;

use crate::{
    domain::chain::TransferResp,
    // infrastructure::swap_client::AggQuoteResp, // AggQuoteResp未使用
    request::{
        api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
        // transaction::{ApproveReq, DepositReq, QuoteReq, SwapReq, WithdrawReq}, // 未使用的transaction类型
    },
    response_vo::{MultisigQueueFeeParams, TransferParams},
};

use wallet_chain_interact::types::ChainPrivateKey;

use wallet_database::entities::api_coin::ApiCoinEntity;

use wallet_database::entities::{
    api_assets::ApiAssetsEntity, multisig_account::MultisigAccountEntity,
    multisig_member::MultisigMemberEntities, multisig_queue::MultisigQueueEntity,
    permission::PermissionEntity,
};
use wallet_transport_backend::{api::BackendApi, response_vo::chain::GasOracle};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum RawTx {
    Tron(
        wallet_chain_interact::tron::operations::RawTransactionParams,
        wallet_chain_interact::BillResourceConsume,
        String,
    ),
    Evm(Vec<u8>, U256),  // eth/bnb/polygon
    Sol(String, String), // solana tx serialized
}

#[async_trait::async_trait]
pub trait Oracle {
    async fn gas_oracle(&self) -> Result<GasOracle, crate::error::service::ServiceError>;

    async fn default_gas_oracle(&self) -> Result<GasOracle, crate::error::service::ServiceError>;
}

#[async_trait::async_trait]
pub trait Tx {
    fn check_min_transfer(
        &self,
        value: &str,
        decimal: u8,
    ) -> Result<U256, crate::error::service::ServiceError> {
        let min = U256::from(1);
        let transfer_amount = unit::convert_to_u256(value, decimal)?;

        if transfer_amount < min {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::AmountLessThanMin,
            ))?;
        }
        Ok(transfer_amount)
    }

    async fn account_resource(
        &self,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, crate::error::service::ServiceError>;

    async fn balance(
        &self,
        addr: &str,
        token: Option<String>,
    ) -> Result<U256, wallet_chain_interact::Error>;

    async fn nonce(&self, addr: &str) -> Result<u64, crate::error::service::ServiceError>;

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error>;

    async fn query_tx_res(
        &self,
        hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>;

    async fn token_symbol(&self, token: &str) -> Result<String, wallet_chain_interact::Error>;

    async fn token_name(&self, token: &str) -> Result<String, wallet_chain_interact::Error>;

    async fn black_address(
        &self,
        token: &str,
        owner: &str,
    ) -> Result<bool, crate::error::service::ServiceError>;

    async fn transfer(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, crate::error::service::ServiceError>;

    async fn estimate_fee(
        &self,
        req: ApiBaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, crate::error::service::ServiceError>;

    async fn build_transfer_raw(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<(String, RawTx, String), crate::error::service::ServiceError>;

    async fn broadcast_transfer(
        &self,
        raw: RawTx,
    ) -> Result<TransferResp, crate::error::service::ServiceError>;

    // async fn approve(
    //     &self,
    //     req: &ApproveReq,
    //     key: ChainPrivateKey,
    //     value: U256,
    // ) -> Result<TransferResp, crate::error::service::ServiceError>;
    //
    // async fn approve_fee(
    //     &self,
    //     req: &ApproveReq,
    //     value: U256,
    //     main_symbol: &str,
    // ) -> Result<String, crate::error::service::ServiceError>;
    //
    // async fn allowance(
    //     &self,
    //     from: &str,
    //     token: &str,
    //     spender: &str,
    // ) -> Result<U256, crate::error::service::ServiceError>;
    //
    // async fn swap_quote(
    //     &self,
    //     req: &QuoteReq,
    //     quote_resp: &AggQuoteResp,
    //     symbol: &str,
    // ) -> Result<(U256, String, String), crate::error::service::ServiceError>;
    //
    // async fn swap(
    //     &self,
    //     req: &SwapReq,
    //     fee: String,
    //     key: ChainPrivateKey,
    // ) -> Result<TransferResp, crate::error::service::ServiceError>;
    //
    // async fn deposit_fee(
    //     &self,
    //     req: DepositReq,
    //     main_coin: &CoinEntity,
    // ) -> Result<(String, String), crate::error::service::ServiceError>;
    //
    // async fn deposit(
    //     &self,
    //     req: &DepositReq,
    //     fee: String,
    //     key: ChainPrivateKey,
    //     value: U256,
    // ) -> Result<TransferResp, crate::error::service::ServiceError>;
    //
    // async fn withdraw_fee(
    //     &self,
    //     req: WithdrawReq,
    //     main_coin: &CoinEntity,
    // ) -> Result<(String, String), crate::error::service::ServiceError>;
    //
    // async fn withdraw(
    //     &self,
    //     req: &WithdrawReq,
    //     fee: String,
    //     key: ChainPrivateKey,
    //     value: U256,
    // ) -> Result<TransferResp, crate::error::service::ServiceError>;
}

#[async_trait::async_trait]
pub trait Multisig {
    async fn multisig_address(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, crate::error::service::ServiceError>;

    async fn deploy_multisig_account(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
        fee_setting: Option<String>,
        key: ChainPrivateKey,
    ) -> Result<(String, String), crate::error::service::ServiceError>;

    async fn deploy_multisig_fee(
        &self,
        account: &MultisigAccountEntity,
        member: MultisigMemberEntities,
        main_symbol: &str,
    ) -> Result<String, crate::error::service::ServiceError>;

    async fn build_multisig_fee(
        &self,
        req: &MultisigQueueFeeParams,
        account: &MultisigAccountEntity,
        decimal: u8,
        token: Option<String>,
        main_symbol: &str,
    ) -> Result<String, crate::error::service::ServiceError>;

    async fn build_multisig_with_account(
        &self,
        req: &TransferParams,
        account: &MultisigAccountEntity,
        assets: &ApiAssetsEntity,
        key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, crate::error::service::ServiceError>;

    async fn build_multisig_with_permission(
        &self,
        req: &TransferParams,
        p: &PermissionEntity,
        coin: &ApiCoinEntity,
    ) -> Result<MultisigTxResp, crate::error::service::ServiceError>;

    async fn sign_fee(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        raw_data: &str,
        main_symbol: &str,
    ) -> Result<String, crate::error::service::ServiceError>;

    async fn sign_multisig_tx(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        key: ChainPrivateKey,
        raw_data: &str,
    ) -> Result<MultisigSignResp, crate::error::service::ServiceError>;

    async fn estimate_multisig_fee(
        &self,
        queue: &MultisigQueueEntity,
        coin: &ApiCoinEntity,
        backend: &BackendApi,
        sign_list: Vec<String>,
        main_symbol: &str,
    ) -> Result<String, crate::error::service::ServiceError>;
}

// 创建一个枚举来包装所有 Tx 实现
// #[enum_dispatch::enum_dispatch(Tx)]
// pub enum ApiTxAdapter {
//     Btc(BtcTx),
//     Doge(DogeTx),
//     Eth(EthTx),
//     Bnb(EthTx),
//     Ltc(LtcTx),
//     Sol(SolTx),
//     Sui(SuiTx),
//     Ton(TonTx),
//     Tron(TronTx),
// }
