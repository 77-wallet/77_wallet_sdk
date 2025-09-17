use crate::{
    domain::{
        api_wallet::adapter::{
            TIME_OUT,
            tx::{Multisig, Oracle, Tx},
        },
        chain::{
            TransferResp, pare_fee_setting,
            swap::{
                EstimateSwapResult, calc_slippage,
                evm_swap::{SwapParams, dexSwap1Call},
            },
        },
        coin::TokenCurrencyGetter,
        multisig::MultisigDomain,
    },
    error::service::ServiceError,
    infrastructure::swap_client::AggQuoteResp,
    request::transaction::{ApproveReq, DepositReq, QuoteReq, SwapReq, WithdrawReq},
    response_vo::{EthereumFeeDetails, FeeDetails, MultisigQueueFeeParams, TransferParams},
};

use crate::request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq};
use alloy::{
    network::TransactionBuilder as _,
    primitives::U256,
    rpc::types::TransactionRequest,
    sol_types::{SolCall as _, SolValue},
};
use std::collections::HashMap;
use wallet_chain_interact::{
    Error, QueryTransactionResult,
    eth::{
        self, EthChain, FeeSetting,
        operations::{
            MultisigAccountOpt, MultisigTransferOpt, TransferOpt,
            erc::{Allowance, Approve, Deposit, Withdraw},
        },
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
    repositories::api_nonce::ApiNonceRepo,
};
use wallet_transport::client::RpcClient;
use wallet_transport_backend::{api::BackendApi, response_vo::chain::GasOracle};
use wallet_types::chain::{chain::ChainCode, network::NetworkKind};
use wallet_utils::{serde_func, unit};

pub(crate) struct EthTx {
    chain_code: ChainCode,
    chain: EthChain,
    provider: eth::Provider,
}

impl EthTx {
    pub(crate) fn new(
        chain_code: ChainCode,
        rpc_url: &str,
        network: NetworkKind,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, wallet_chain_interact::Error> {
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let rpc_client = RpcClient::new(rpc_url, header_opt.clone(), timeout)?;
        let provider = eth::Provider::new(rpc_client)?;
        let eth_chain = EthChain::new(provider, network, chain_code)?;
        let rpc_client1 = RpcClient::new(rpc_url, header_opt, timeout)?;
        let provider1 = eth::Provider::new(rpc_client1)?;
        Ok(Self { chain_code, chain: eth_chain, provider: provider1 })
    }

    async fn estimate_swap(
        &self,
        swap_params: SwapParams,
    ) -> Result<EstimateSwapResult<FeeSetting>, ServiceError> {
        let tx = self.build_base_swap_tx(&swap_params)?;

        // estimate_fee
        let gas_limit = self.chain.provider.estimate_gas(tx.clone()).await?;
        let tx = tx.with_gas_limit(gas_limit.to::<u64>());

        let gas_price = self.chain.provider.gas_price().await?;
        let mut fee = FeeSetting::new_with_price(gas_price);
        fee.gas_limit = gas_limit;

        let result = self.chain.provider.eth_call(tx).await?;
        let bytes = wallet_utils::hex_func::hex_decode(&result[2..])?;

        let (amount_in, amount_out): (U256, U256) = <(U256, U256)>::abi_decode_params(&bytes, true)
            .map_err(|e| crate::error::service::ServiceError::AggregatorError {
                code: -1,
                agg_code: 0,
                msg: e.to_string(),
            })?;

        let resp = EstimateSwapResult { amount_in, amount_out, consumer: fee };
        Ok(resp)
    }

    async fn swap_base_transfer(
        &self,
        swap_params: &SwapParams,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferResp, crate::error::service::ServiceError> {
        let fee = pare_fee_setting(fee.as_str())?;
        let transfer_fee = fee.transaction_fee();

        let tx = self.build_base_swap_tx(&swap_params)?;
        let tx = self.chain.provider.set_transaction_fee(tx, fee, self.chain.chain_code).await?;

        let tx_hash = self.chain.provider.send_raw_transaction(tx, &key, None).await?;

        Ok(TransferResp::new(
            tx_hash,
            unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
        ))
    }

    fn build_base_swap_tx(
        &self,
        swap_params: &SwapParams,
    ) -> Result<TransactionRequest, crate::error::service::ServiceError> {
        let call_value = dexSwap1Call::try_from((swap_params, ChainCode::Ethereum))?;

        let tx = TransactionRequest::default()
            .from(swap_params.recipient)
            .to(swap_params.aggregator_addr)
            .with_input(call_value.abi_encode());

        // from token 如果是主币添加默认的币
        let tx =
            if swap_params.token_in.is_zero() { tx.with_value(swap_params.amount_in) } else { tx };

        Ok(tx)
    }

    pub async fn check_eth_balance(
        &self,
        from: &str,
        balance: U256,
        token: Option<&str>,
        transfer_amount: U256,
    ) -> Result<U256, ServiceError> {
        let cost_main = match token {
            Some(token) => {
                let token_balance = self.chain.balance(from, Some(token.to_string())).await?;
                if token_balance < transfer_amount {
                    return Err(crate::error::business::BusinessError::Chain(
                        crate::error::business::chain::ChainError::InsufficientBalance,
                    ))?;
                }
                balance
            }
            None => {
                if balance < transfer_amount {
                    return Err(crate::error::business::BusinessError::Chain(
                        crate::error::business::chain::ChainError::InsufficientBalance,
                    ))?;
                }
                balance - transfer_amount
            }
        };
        Ok(cost_main)
    }
}

#[async_trait::async_trait]
impl Oracle for EthTx {
    async fn gas_oracle(&self) -> Result<GasOracle, ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let gas_oracle = backend.gas_oracle(&self.chain_code.to_string()).await;

        match gas_oracle {
            Ok(gas_oracle) => Ok(gas_oracle),
            Err(_) => {
                // unit is wei need to gwei
                let eth_fee = self.chain.provider.get_default_fee().await?;

                let propose = eth_fee.base_fee + eth_fee.priority_fee_per_gas;
                let propose = unit::format_to_string(propose, eth::consts::ETH_GWEI)?;
                let base = unit::format_to_string(eth_fee.base_fee, eth::consts::ETH_GWEI)?;

                let gas_oracle = GasOracle {
                    safe_gas_price: None,
                    propose_gas_price: Some(propose),
                    fast_gas_price: None,
                    suggest_base_fee: Some(base),
                    gas_used_ratio: None,
                };

                Ok(gas_oracle)
            }
        }
    }

    async fn default_gas_oracle(&self) -> Result<GasOracle, ServiceError> {
        let eth_fee = self.chain.provider.get_default_fee().await?;

        let propose = eth_fee.base_fee + eth_fee.priority_fee_per_gas;
        let propose = unit::format_to_string(propose, eth::consts::ETH_GWEI)?;
        let base = unit::format_to_string(eth_fee.base_fee, eth::consts::ETH_GWEI)?;

        let gas_oracle = GasOracle {
            safe_gas_price: None,
            propose_gas_price: Some(propose),
            fast_gas_price: None,
            suggest_base_fee: Some(base),
            gas_used_ratio: None,
        };

        Ok(gas_oracle)
    }
}

#[async_trait::async_trait]
impl Tx for EthTx {
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
        let res = self.chain.black_address(token, owner).await?;
        Ok(res)
    }

    async fn transfer(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        tracing::info!("transfer ------------------- 11:");
        let transfer_amount = self.check_min_transfer(&params.base.value, params.base.decimals)?;
        let from = params.base.from.as_str();
        let to = params.base.to.as_str();
        tracing::info!(from=%from,to=%to,value=%transfer_amount, "transfer ------------------- 12");

        // 获取主币余额
        let eth_balance =
            self.chain.balance(&params.base.from, params.base.token_address.clone()).await?;
        tracing::info!(eth_balance=%eth_balance, "transfer ------------------- 13");
        // check balance
        let remain_balance = self
            .check_eth_balance(
                &params.base.from,
                eth_balance,
                params.base.token_address.as_deref(),
                transfer_amount,
            )
            .await?;

        // 预估gas
        tracing::info!(eth_balance=%eth_balance, "transfer ------------------- 14");
        let transfer_opt =
            TransferOpt::new(from, to, transfer_amount, params.base.token_address.clone())?;
        tracing::info!(eth_balance=%eth_balance, "transfer ------------------- 15");
        let rc = self.chain.estimate_gas(transfer_opt).await?;
        // check transaction_fee
        if remain_balance < rc.consume {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance,
            ))?;
        }

        let gas_oracle = self.gas_oracle().await?;
        let propose_gas_price = gas_oracle.propose_gas_price;
        if propose_gas_price.is_none() {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::GasOracle,
            ))?;
        }
        let price = unit::convert_to_u256(&propose_gas_price.unwrap(), params.base.decimals)?;
        let fee_setting = FeeSetting::new_with_price(price);
        let fee = fee_setting.transaction_fee();
        let transfer_opt =
            TransferOpt::new(from, to, transfer_amount, params.base.token_address.clone())?;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut nonce =
            ApiNonceRepo::get_api_nonce(&pool, from, &params.base.chain_code).await? as u64;
        if nonce == 0 {
            let ol_nonce = self.provider.nonce(&from).await?;
            if ol_nonce > nonce {
                nonce = ol_nonce;
            }
        }
        let tx_hash = self
            .chain
            .exec_transaction(transfer_opt, fee_setting, private_key, Some(nonce))
            .await?;

        ApiNonceRepo::upsert_and_get_api_nonce(&pool, from, &params.base.chain_code, nonce as i32)
            .await?;

        tracing::info!("transfer ------------------- 16: {tx_hash}");
        Ok(TransferResp::new(tx_hash, unit::format_to_string(fee, eth::consts::ETH_DECIMAL)?))
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
        let value = unit::convert_to_u256(&req.value, req.decimals)?;
        let balance = self.chain.balance(&req.from, req.token_address.clone()).await?;
        if balance < value {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientBalance,
            ))?;
        }

        let gas_oracle = self.gas_oracle().await?;
        let params = TransferOpt::new(&req.from, &req.to, value, req.token_address)?;
        let fee = self.chain.estimate_gas(params).await?;
        let fee =
            FeeDetails::try_from((gas_oracle, fee.consume))?.to_resp(token_currency, currency);

        let res = wallet_utils::serde_func::serde_to_string(&fee)?;
        Ok(res)
    }

    async fn approve(
        &self,
        req: &ApproveReq,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

        // 使用默认的手续费配置
        let gas_price = self.chain.provider.gas_price().await?;
        let fee_setting = FeeSetting::new_with_price(gas_price);

        let fee = fee_setting.transaction_fee();

        // exec tx
        let tx_hash = self.chain.exec_transaction(approve, fee_setting, key, None).await?;

        Ok(TransferResp::new(tx_hash, unit::format_to_string(fee, eth::consts::ETH_DECIMAL)?))
    }

    async fn approve_fee(
        &self,
        req: &ApproveReq,
        value: U256,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency = {
            let currency = crate::app_state::APP_STATE.read().await;
            currency.currency().to_string()
        };

        let token_currency =
            TokenCurrencyGetter::get_currency(&currency, &req.chain_code, main_symbol, None)
                .await?;

        let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

        let fee = self.chain.estimate_gas(approve).await?;

        let gas_oracle = self.default_gas_oracle().await?;
        let fee =
            FeeDetails::try_from((gas_oracle, fee.consume))?.to_resp(token_currency, &currency);

        let res = wallet_utils::serde_func::serde_to_string(&fee)?;
        Ok(res)
    }

    async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<U256, ServiceError> {
        let approve = Allowance::new(from, token, spender)?;
        let amount = self.chain.eth_call::<_, U256>(approve).await?;
        Ok(amount)
    }

    async fn swap_quote(
        &self,
        req: &QuoteReq,
        quote_resp: &AggQuoteResp,
        symbol: &str,
    ) -> Result<(U256, String, String), ServiceError> {
        let amount_out = quote_resp.amount_out_u256()?;

        // 考虑滑点计算最小金额
        let min_amount_out =
            calc_slippage(amount_out, req.get_slippage(quote_resp.default_slippage));

        let currency = {
            let currency = crate::app_state::APP_STATE.read().await;
            currency.currency().to_string()
        };

        let token_currency =
            TokenCurrencyGetter::get_currency(&currency, &req.chain_code, symbol, None).await?;

        let swap_params = SwapParams {
            aggregator_addr: req.aggregator_address()?,
            amount_in: req.amount_in_u256()?,
            min_amount_out,
            recipient: wallet_utils::address::parse_eth_address(&req.recipient)?,
            token_in: SwapParams::eth_parse_or_zero_addr(&req.token_in.token_addr)?,
            token_out: SwapParams::eth_parse_or_zero_addr(&req.token_out.token_addr)?,
            dex_router: quote_resp.dex_route_list.clone(),
            allow_partial_fill: req.allow_partial_fill,
        };

        let resp = self.estimate_swap(swap_params).await?;

        let gas_oracle = self.default_gas_oracle().await?;
        let fee = FeeDetails::try_from((gas_oracle, resp.consumer.gas_limit.to::<i64>()))?
            .to_resp(token_currency, &currency);

        // 消耗的资源
        let consumer = wallet_utils::serde_func::serde_to_string(&fee.data[0].fee_setting)?;
        // 具体的手续费结构
        let fee = wallet_utils::serde_func::serde_to_string(&fee)?;

        Ok((resp.amount_out, consumer, fee))
    }

    async fn swap(
        &self,
        req: &SwapReq,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        let swap_params = SwapParams::try_from(req)?;
        self.swap_base_transfer(&swap_params, fee, key).await
    }

    async fn deposit_fee(
        &self,
        req: DepositReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        let currency = {
            let currency = crate::app_state::APP_STATE.read().await;
            currency.currency().to_string()
        };

        let token_currency =
            TokenCurrencyGetter::get_currency(&currency, &req.chain_code, &main_coin.symbol, None)
                .await?;
        let value = wallet_utils::unit::convert_to_u256(&req.amount, main_coin.decimals)?;

        let approve = Deposit::new(&req.from, &req.token, value)?;

        let resource = self.chain.estimate_gas(approve).await?;

        let gas_oracle = self.default_gas_oracle().await?;
        let fee = FeeDetails::try_from((gas_oracle, resource.consume))?
            .to_resp(token_currency, &currency);

        let consumer = wallet_utils::serde_func::serde_to_string(&fee.data[0].fee_setting)?;
        let fee = wallet_utils::serde_func::serde_to_string(&fee)?;

        Ok((consumer, fee))
    }

    async fn deposit(
        &self,
        req: &DepositReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let approve = Deposit::new(&req.from, &req.token, value)?;

        // 使用默认的手续费配置
        let fee_setting = pare_fee_setting(fee.as_str())?;
        let transfer_fee = fee_setting.transaction_fee();

        // exec tx
        let tx_hash = self.chain.exec_transaction(approve, fee_setting, key, None).await?;

        Ok(TransferResp::new(
            tx_hash,
            unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
        ))
    }

    async fn withdraw_fee(
        &self,
        req: WithdrawReq,
        main_coin: &CoinEntity,
    ) -> Result<(String, String), ServiceError> {
        let currency = {
            let currency = crate::app_state::APP_STATE.read().await;
            currency.currency().to_string()
        };

        let token_currency =
            TokenCurrencyGetter::get_currency(&currency, &req.chain_code, &main_coin.symbol, None)
                .await?;

        let value = wallet_utils::unit::convert_to_u256(&req.amount, main_coin.decimals)?;

        let withdraw = Withdraw::new(&req.from, &req.token, value)?;

        let resource = self.chain.estimate_gas(withdraw).await?;
        let gas_oracle = self.default_gas_oracle().await?;
        let fee = FeeDetails::try_from((gas_oracle, resource.consume))?
            .to_resp(token_currency, &currency);

        let consumer = wallet_utils::serde_func::serde_to_string(&fee.data[0].fee_setting)?;
        let fee = wallet_utils::serde_func::serde_to_string(&fee)?;

        Ok((consumer, fee))
    }

    async fn withdraw(
        &self,
        req: &WithdrawReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let withdraw = Withdraw::new(&req.from, &req.token, value)?;

        // 使用默认的手续费配置
        let fee_setting = pare_fee_setting(fee.as_str())?;
        let transfer_fee = fee_setting.transaction_fee();

        // exec tx
        let tx_hash = self.chain.exec_transaction(withdraw, fee_setting, key, None).await?;

        Ok(TransferResp::new(
            tx_hash,
            unit::format_to_string(transfer_fee, eth::consts::ETH_DECIMAL)?,
        ))
    }
}

#[async_trait::async_trait]
impl Multisig for EthTx {
    async fn multisig_address(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, ServiceError> {
        let params = MultisigAccountOpt::new(&account.initiator_addr, account.threshold)?
            .with_nonce()
            .with_owners(member.get_owner_str_vec())?;
        Ok(self.chain.multisig_account(params).await?)
    }

    async fn deploy_multisig_account(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
        fee_setting: Option<String>,
        key: ChainPrivateKey,
    ) -> Result<(String, String), ServiceError> {
        let params = MultisigAccountOpt::new(&account.initiator_addr, account.threshold)?
            .set_nonce(&account.salt)?
            .with_owners(member.get_owner_str_vec())?;

        let fee_setting: EthereumFeeDetails = serde_func::serde_from_str(&fee_setting.unwrap())?;
        let fee_setting = FeeSetting::try_from(fee_setting)?;

        // check transaction_fee
        let balance = self.chain.balance(&account.initiator_addr, None).await?;
        if balance < fee_setting.transaction_fee() {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance,
            ))?;
        }

        let tx_hash = self.chain.exec_transaction(params, fee_setting, key, None).await?;
        Ok((tx_hash, "".to_string()))
    }

    async fn deploy_multisig_fee(
        &self,
        account: &MultisigAccountEntity,
        member: MultisigMemberEntities,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency_lock = crate::app_state::APP_STATE.read().await;
        let currency = currency_lock.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &account.chain_code, main_symbol, None)
                .await?;

        let owner = member.get_owner_str_vec();
        let params = MultisigAccountOpt::new(&account.initiator_addr, account.threshold)?
            .with_nonce()
            .with_owners(owner)?;

        let gas_limit = self.chain.estimate_gas(params).await?;

        let gas_oracle = self.gas_oracle().await?;

        let fee = FeeDetails::try_from((gas_oracle, gas_limit.consume))?
            .to_resp(token_currency, currency);
        Ok(wallet_utils::serde_func::serde_to_string(&fee)?)
    }

    async fn build_multisig_fee(
        &self,
        _req: &MultisigQueueFeeParams,
        _account: &MultisigAccountEntity,
        _decimal: u8,
        _token: Option<String>,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok("".to_string())
    }

    async fn build_multisig_with_account(
        &self,
        req: &TransferParams,
        _account: &MultisigAccountEntity,
        assets: &ApiAssetsEntity,
        _key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, ServiceError> {
        let decimal = assets.decimals;
        let token = assets.token_address();

        let value = self.check_min_transfer(&req.value, decimal)?;
        let balance = self.chain.balance(&req.from, token.clone()).await?;
        let _ = self.check_eth_balance(&req.from, balance, token.as_deref(), value).await?;

        let params = MultisigTransferOpt::new(&req.from, &req.to, value)?.with_token(token)?;

        Ok(self.chain.build_multisig_tx(params).await?)
    }

    async fn build_multisig_with_permission(
        &self,
        _req: &TransferParams,
        _p: &PermissionEntity,
        _coin: &CoinEntity,
    ) -> Result<MultisigTxResp, ServiceError> {
        Err(crate::error::business::BusinessError::Permission(
            crate::error::business::permission::PermissionError::UnSupportPermissionChain,
        )
        .into())
    }

    async fn sign_fee(
        &self,
        _account: &MultisigAccountEntity,
        _address: &str,
        _raw_data: &str,
        _main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok(" ".to_string())
    }

    async fn sign_multisig_tx(
        &self,
        _account: &MultisigAccountEntity,
        _address: &str,
        key: ChainPrivateKey,
        raw_data: &str,
    ) -> Result<MultisigSignResp, ServiceError> {
        use std::str::FromStr as _;
        let operate = eth::operations::MultisigPayloadOpt::from_str(raw_data)?;
        Ok(operate.sign_message(key)?)
    }

    async fn estimate_multisig_fee(
        &self,
        queue: &MultisigQueueEntity,
        coin: &CoinEntity,
        _backend: &BackendApi,
        sign_list: Vec<String>,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &queue.chain_code, main_symbol, None)
                .await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let value = unit::convert_to_u256(&queue.value, coin.decimals)?;
        let multisig_account =
            MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;

        let gas_oracle = self.gas_oracle().await?;

        let params = MultisigTransferOpt::new(&queue.from_addr, &queue.to_addr, value)?
            .with_token(coin.token_address())?
            .exec_params(
                &multisig_account.initiator_addr,
                queue.raw_data.clone(),
                sign_list.join(""),
            )?;

        let fee = self.chain.estimate_gas(params).await?;
        let fee =
            FeeDetails::try_from((gas_oracle, fee.consume))?.to_resp(token_currency, currency);

        Ok(wallet_utils::serde_func::serde_to_string(&fee)?)
    }
}
