use crate::{
    domain::api_wallet::adapter::{
        btc_tx::BtcTx, doge_tx::DogeTx, eth_tx::EthTx, ltx_tx::LtcTx, sol_tx::SolTx, sui_tx::SuiTx,
        ton_tx::TonTx, tron_tx::TronTx, Tx,
    },
    ServiceError,
};
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;

// 创建一个枚举来包装所有 Tx 实现
pub enum ApiTxAdapter {
    Btc(BtcTx),
    Doge(DogeTx),
    Eth(EthTx),
    Bnb(EthTx),
    Ltc(LtcTx),
    Sol(SolTx),
    Sui(SuiTx),
    Ton(TonTx),
    Tron(TronTx),
}

// 为枚举实现 Tx trait
#[async_trait::async_trait]
impl Tx for ApiTxAdapter {
    async fn account_resource(
        &self,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, ServiceError> {
        match self {
            Self::Btc(tx) => tx.account_resource(owner_address).await,
            Self::Doge(tx) => tx.account_resource(owner_address).await,
            Self::Eth(tx) => tx.account_resource(owner_address).await,
            Self::Ltc(tx) => tx.account_resource(owner_address).await,
            Self::Sol(tx) => tx.account_resource(owner_address).await,
            Self::Sui(tx) => tx.account_resource(owner_address).await,
            Self::Ton(tx) => tx.account_resource(owner_address).await,
            Self::Tron(tx) => tx.account_resource(owner_address).await,
            Self::Bnb(tx) => tx.account_resource(owner_address).await,
        }
    }

    async fn balance(
        &self,
        addr: &str,
        token: Option<String>,
    ) -> Result<alloy::primitives::U256, wallet_chain_interact::Error> {
        match self {
            Self::Btc(tx) => tx.balance(addr, token).await,
            Self::Doge(tx) => tx.balance(addr, token).await,
            Self::Eth(tx) => tx.balance(addr, token).await,
            Self::Ltc(tx) => tx.balance(addr, token).await,
            Self::Sol(tx) => tx.balance(addr, token).await,
            Self::Sui(tx) => tx.balance(addr, token).await,
            Self::Ton(tx) => tx.balance(addr, token).await,
            Self::Tron(tx) => tx.balance(addr, token).await,
            Self::Bnb(tx) => tx.balance(addr, token).await,
        }
    }

    async fn block_num(&self) -> Result<u64, wallet_chain_interact::Error> {
        match self {
            Self::Btc(tx) => tx.block_num().await,
            Self::Doge(tx) => tx.block_num().await,
            Self::Eth(tx) => tx.block_num().await,
            Self::Ltc(tx) => tx.block_num().await,
            Self::Sol(tx) => tx.block_num().await,
            Self::Sui(tx) => tx.block_num().await,
            Self::Ton(tx) => tx.block_num().await,
            Self::Tron(tx) => tx.block_num().await,
            Self::Bnb(tx) => tx.block_num().await,
        }
    }

    async fn query_tx_res(
        &self,
        hash: &str,
    ) -> Result<Option<wallet_chain_interact::QueryTransactionResult>, wallet_chain_interact::Error>
    {
        match self {
            Self::Btc(tx) => tx.query_tx_res(hash).await,
            Self::Doge(tx) => tx.query_tx_res(hash).await,
            Self::Eth(tx) => tx.query_tx_res(hash).await,
            Self::Ltc(tx) => tx.query_tx_res(hash).await,
            Self::Sol(tx) => tx.query_tx_res(hash).await,
            Self::Sui(tx) => tx.query_tx_res(hash).await,
            Self::Ton(tx) => tx.query_tx_res(hash).await,
            Self::Tron(tx) => tx.query_tx_res(hash).await,
            Self::Bnb(tx) => tx.query_tx_res(hash).await,
        }
    }

    async fn decimals(&self, token: &str) -> Result<u8, wallet_chain_interact::Error> {
        match self {
            Self::Btc(tx) => tx.decimals(token).await,
            Self::Doge(tx) => tx.decimals(token).await,
            Self::Eth(tx) => tx.decimals(token).await,
            Self::Ltc(tx) => tx.decimals(token).await,
            Self::Sol(tx) => tx.decimals(token).await,
            Self::Sui(tx) => tx.decimals(token).await,
            Self::Ton(tx) => tx.decimals(token).await,
            Self::Tron(tx) => tx.decimals(token).await,
            Self::Bnb(tx) => tx.decimals(token).await,
        }
    }

    async fn token_symbol(&self, token: &str) -> Result<String, wallet_chain_interact::Error> {
        match self {
            Self::Btc(tx) => tx.token_symbol(token).await,
            Self::Doge(tx) => tx.token_symbol(token).await,
            Self::Eth(tx) => tx.token_symbol(token).await,
            Self::Ltc(tx) => tx.token_symbol(token).await,
            Self::Sol(tx) => tx.token_symbol(token).await,
            Self::Sui(tx) => tx.token_symbol(token).await,
            Self::Ton(tx) => tx.token_symbol(token).await,
            Self::Tron(tx) => tx.token_symbol(token).await,
            Self::Bnb(tx) => tx.token_symbol(token).await,
        }
    }

    async fn token_name(&self, token: &str) -> Result<String, wallet_chain_interact::Error> {
        match self {
            Self::Btc(tx) => tx.token_name(token).await,
            Self::Doge(tx) => tx.token_name(token).await,
            Self::Eth(tx) => tx.token_name(token).await,
            Self::Ltc(tx) => tx.token_name(token).await,
            Self::Sol(tx) => tx.token_name(token).await,
            Self::Sui(tx) => tx.token_name(token).await,
            Self::Ton(tx) => tx.token_name(token).await,
            Self::Tron(tx) => tx.token_name(token).await,
            Self::Bnb(tx) => tx.token_name(token).await,
        }
    }

    async fn black_address(&self, token: &str, owner: &str) -> Result<bool, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.black_address(token, owner).await,
            Self::Doge(tx) => tx.black_address(token, owner).await,
            Self::Eth(tx) => tx.black_address(token, owner).await,
            Self::Ltc(tx) => tx.black_address(token, owner).await,
            Self::Sol(tx) => tx.black_address(token, owner).await,
            Self::Sui(tx) => tx.black_address(token, owner).await,
            Self::Ton(tx) => tx.black_address(token, owner).await,
            Self::Tron(tx) => tx.black_address(token, owner).await,
            Self::Bnb(tx) => tx.black_address(token, owner).await,
        }
    }

    async fn transfer(
        &self,
        params: &crate::request::transaction::TransferReq,
        private_key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<crate::domain::chain::TransferResp, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.transfer(params, private_key).await,
            Self::Doge(tx) => tx.transfer(params, private_key).await,
            Self::Eth(tx) => tx.transfer(params, private_key).await,
            Self::Ltc(tx) => tx.transfer(params, private_key).await,
            Self::Sol(tx) => tx.transfer(params, private_key).await,
            Self::Sui(tx) => tx.transfer(params, private_key).await,
            Self::Ton(tx) => tx.transfer(params, private_key).await,
            Self::Tron(tx) => tx.transfer(params, private_key).await,
            Self::Bnb(tx) => tx.transfer(params, private_key).await,
        }
    }

    async fn estimate_fee(
        &self,
        req: crate::request::transaction::BaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Doge(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Eth(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Ltc(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Sol(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Sui(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Ton(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Tron(tx) => tx.estimate_fee(req, main_symbol).await,
            Self::Bnb(tx) => tx.estimate_fee(req, main_symbol).await,
        }
    }

    async fn approve(
        &self,
        req: &crate::request::transaction::ApproveReq,
        key: wallet_chain_interact::types::ChainPrivateKey,
        value: alloy::primitives::U256,
    ) -> Result<crate::domain::chain::TransferResp, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.approve(req, key, value).await,
            Self::Doge(tx) => tx.approve(req, key, value).await,
            Self::Eth(tx) => tx.approve(req, key, value).await,
            Self::Ltc(tx) => tx.approve(req, key, value).await,
            Self::Sol(tx) => tx.approve(req, key, value).await,
            Self::Sui(tx) => tx.approve(req, key, value).await,
            Self::Ton(tx) => tx.approve(req, key, value).await,
            Self::Tron(tx) => tx.approve(req, key, value).await,
            Self::Bnb(tx) => tx.approve(req, key, value).await,
        }
    }

    async fn approve_fee(
        &self,
        req: &crate::request::transaction::ApproveReq,
        value: alloy::primitives::U256,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Doge(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Eth(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Ltc(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Sol(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Sui(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Ton(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Tron(tx) => tx.approve_fee(req, value, main_symbol).await,
            Self::Bnb(tx) => tx.approve_fee(req, value, main_symbol).await,
        }
    }

    async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<alloy::primitives::U256, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.allowance(from, token, spender).await,
            Self::Doge(tx) => tx.allowance(from, token, spender).await,
            Self::Eth(tx) => tx.allowance(from, token, spender).await,
            Self::Ltc(tx) => tx.allowance(from, token, spender).await,
            Self::Sol(tx) => tx.allowance(from, token, spender).await,
            Self::Sui(tx) => tx.allowance(from, token, spender).await,
            Self::Ton(tx) => tx.allowance(from, token, spender).await,
            Self::Tron(tx) => tx.allowance(from, token, spender).await,
            Self::Bnb(tx) => tx.allowance(from, token, spender).await,
        }
    }

    async fn swap_quote(
        &self,
        req: &crate::request::transaction::QuoteReq,
        quote_resp: &crate::infrastructure::swap_client::AggQuoteResp,
        symbol: &str,
    ) -> Result<(alloy::primitives::U256, String, String), crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Doge(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Eth(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Ltc(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Sol(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Sui(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Ton(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Tron(tx) => tx.swap_quote(req, quote_resp, symbol).await,
            Self::Bnb(tx) => tx.swap_quote(req, quote_resp, symbol).await,
        }
    }

    async fn swap(
        &self,
        req: &crate::request::transaction::SwapReq,
        fee: String,
        key: wallet_chain_interact::types::ChainPrivateKey,
    ) -> Result<crate::domain::chain::TransferResp, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.swap(req, fee, key).await,
            Self::Doge(tx) => tx.swap(req, fee, key).await,
            Self::Eth(tx) => tx.swap(req, fee, key).await,
            Self::Ltc(tx) => tx.swap(req, fee, key).await,
            Self::Sol(tx) => tx.swap(req, fee, key).await,
            Self::Sui(tx) => tx.swap(req, fee, key).await,
            Self::Ton(tx) => tx.swap(req, fee, key).await,
            Self::Tron(tx) => tx.swap(req, fee, key).await,
            Self::Bnb(tx) => tx.swap(req, fee, key).await,
        }
    }

    async fn deposit_fee(
        &self,
        req: crate::request::transaction::DepositReq,
        main_coin: &wallet_database::entities::coin::CoinEntity,
    ) -> Result<(String, String), crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Doge(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Eth(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Ltc(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Sol(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Sui(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Ton(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Tron(tx) => tx.deposit_fee(req, main_coin).await,
            Self::Bnb(tx) => tx.deposit_fee(req, main_coin).await,
        }
    }

    async fn deposit(
        &self,
        req: &crate::request::transaction::DepositReq,
        fee: String,
        key: wallet_chain_interact::types::ChainPrivateKey,
        value: alloy::primitives::U256,
    ) -> Result<crate::domain::chain::TransferResp, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.deposit(req, fee, key, value).await,
            Self::Doge(tx) => tx.deposit(req, fee, key, value).await,
            Self::Eth(tx) => tx.deposit(req, fee, key, value).await,
            Self::Ltc(tx) => tx.deposit(req, fee, key, value).await,
            Self::Sol(tx) => tx.deposit(req, fee, key, value).await,
            Self::Sui(tx) => tx.deposit(req, fee, key, value).await,
            Self::Ton(tx) => tx.deposit(req, fee, key, value).await,
            Self::Tron(tx) => tx.deposit(req, fee, key, value).await,
            Self::Bnb(tx) => tx.deposit(req, fee, key, value).await,
        }
    }

    async fn withdraw_fee(
        &self,
        req: crate::request::transaction::WithdrawReq,
        main_coin: &wallet_database::entities::coin::CoinEntity,
    ) -> Result<(String, String), crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Doge(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Eth(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Ltc(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Sol(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Sui(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Ton(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Tron(tx) => tx.withdraw_fee(req, main_coin).await,
            Self::Bnb(tx) => tx.withdraw_fee(req, main_coin).await,
        }
    }

    async fn withdraw(
        &self,
        req: &crate::request::transaction::WithdrawReq,
        fee: String,
        key: wallet_chain_interact::types::ChainPrivateKey,
        value: alloy::primitives::U256,
    ) -> Result<crate::domain::chain::TransferResp, crate::ServiceError> {
        match self {
            Self::Btc(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Doge(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Eth(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Ltc(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Sol(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Sui(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Ton(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Tron(tx) => tx.withdraw(req, fee, key, value).await,
            Self::Bnb(tx) => tx.withdraw(req, fee, key, value).await,
        }
    }
}
