use super::{eth_tx, ton_tx, tron_tx, TIME_OUT};
use crate::{
    dispatch,
    domain::{
        self,
        chain::{
            pare_fee_setting,
            swap::{calc_slippage, evm_swap::SwapParams, EstimateSwapResult},
            transaction::{ChainTransDomain, DEFAULT_UNITS},
            TransferResp,
        },
        swap_client::AggQuoteResp,
    },
    request::transaction::{self, QuoteReq, SwapReq},
    response_vo::{self, FeeDetails, TronFeeDetails},
};
use alloy::primitives::U256;
use std::collections::HashMap;
use wallet_chain_interact::{
    self as chain,
    btc::{self},
    dog, eth, ltc,
    sol::{self, operations::SolInstructionOperation},
    sui, ton,
    tron::{
        self,
        operations::{TronConstantOperation as _, TronTxOperation},
    },
    types::ChainPrivateKey,
    BillResourceConsume,
};
use wallet_transport::client::{HttpClient, RpcClient};
use wallet_types::chain::{
    address::r#type::{DogAddressType, LtcAddressType, TonAddressType},
    chain::ChainCode as ChainType,
};
use wallet_utils::unit;

pub enum TransactionAdapter {
    BitCoin(chain::btc::BtcChain),
    Ethereum(chain::eth::EthChain),
    Solana(chain::sol::SolanaChain),
    Tron(chain::tron::TronChain),
    Ltc(chain::ltc::LtcChain),
    Doge(chain::dog::DogChain),
    Ton(chain::ton::chain::TonChain),
    Sui(chain::sui::SuiChain),
}

impl TransactionAdapter {
    pub fn new(
        chain_code: ChainType,
        rpc_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<TransactionAdapter, chain::Error> {
        let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        match chain_code {
            ChainType::Bitcoin => {
                let config = chain::btc::provider::ProviderConfig {
                    rpc_url: rpc_url.to_string(),
                    rpc_auth: None,
                    http_url: rpc_url.to_string(),
                    http_api_key: None,
                };
                let btc_chain = chain::btc::BtcChain::new(config, network, header_opt, timeout)?;
                Ok(TransactionAdapter::BitCoin(btc_chain))
            }
            ChainType::Ethereum | ChainType::BnbSmartChain => {
                let rpc_client = RpcClient::new(rpc_url, header_opt, timeout)?;
                let provider = eth::Provider::new(rpc_client)?;
                let eth_chain = chain::eth::EthChain::new(provider, network, chain_code)?;
                Ok(TransactionAdapter::Ethereum(eth_chain))
            }
            ChainType::Solana => {
                let rpc_client = RpcClient::new(rpc_url, header_opt, timeout)?;
                let provider = sol::Provider::new(rpc_client)?;
                let sol_chain = chain::sol::SolanaChain::new(provider)?;
                Ok(TransactionAdapter::Solana(sol_chain))
            }
            ChainType::Tron => {
                let http_client = HttpClient::new(rpc_url, header_opt, timeout)?;
                let provider = tron::Provider::new(http_client)?;

                let tron_chain = chain::tron::TronChain::new(provider)?;
                Ok(TransactionAdapter::Tron(tron_chain))
            }
            ChainType::Dogcoin => {
                let config = chain::dog::provider::ProviderConfig {
                    rpc_url: rpc_url.to_string(),
                    rpc_auth: None,
                    http_url: rpc_url.to_string(),
                    http_api_key: None,
                    access_key: None,
                };

                let doge = chain::dog::DogChain::new(config, network, header_opt, timeout)?;
                Ok(TransactionAdapter::Doge(doge))
            }
            ChainType::Litecoin => {
                let config = chain::ltc::provider::ProviderConfig {
                    rpc_url: rpc_url.to_string(),
                    rpc_auth: None,
                    http_url: rpc_url.to_string(),
                    http_api_key: None,
                    access_key: None,
                };

                let doge = chain::ltc::LtcChain::new(config, network, header_opt, timeout)?;
                Ok(TransactionAdapter::Ltc(doge))
            }
            ChainType::Ton => {
                let http_client = HttpClient::new(rpc_url, header_opt, timeout)?;
                let provider = ton::provider::Provider::new(http_client);

                let ton = chain::ton::chain::TonChain::new(provider)?;

                Ok(TransactionAdapter::Ton(ton))
            }
            ChainType::Sui => {
                let rpc = RpcClient::new(rpc_url, header_opt, timeout)?;

                let provider = sui::Provider::new(rpc);
                Ok(TransactionAdapter::Sui(sui::SuiChain::new(provider)?))
            }
        }
    }
}

impl TransactionAdapter {
    pub async fn balance(&self, addr: &str, token: Option<String>) -> Result<U256, chain::Error> {
        dispatch!(self, balance, addr, token)
    }

    pub async fn block_num(&self) -> Result<u64, chain::Error> {
        dispatch!(self, block_num,)
    }

    pub async fn query_tx_res(
        &self,
        hash: &str,
    ) -> Result<Option<chain::QueryTransactionResult>, chain::Error> {
        dispatch!(self, query_tx_res, hash)
    }

    pub async fn decimals(&self, token: &str) -> Result<u8, chain::Error> {
        dispatch!(self, decimals, token)
    }

    pub async fn token_symbol(&self, token: &str) -> Result<String, chain::Error> {
        dispatch!(self, token_symbol, token)
    }

    pub async fn token_name(&self, token: &str) -> Result<String, chain::Error> {
        dispatch!(self, token_name, token)
    }

    // Check if an address is a blacklisted address
    pub async fn black_address(
        &self,
        chain_code: ChainType,
        token: &str,
        owner: &str,
    ) -> Result<bool, crate::ServiceError> {
        match chain_code {
            ChainType::Ethereum | ChainType::Solana | ChainType::Tron => match self {
                Self::Ethereum(chain) => Ok(chain.black_address(token, owner).await?),
                Self::Solana(chain) => Ok(chain.black_address(token, owner).await?),
                Self::Tron(chain) => Ok(chain.black_address(token, owner).await?),
                _ => Ok(false),
            },

            _ => Ok(false),
        }
    }

    // 返回交易hash以及交易消耗的资源
    pub async fn transfer(
        &self,
        params: &transaction::TransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransferResp, crate::ServiceError> {
        let transfer_amount =
            ChainTransDomain::check_min_transfer(&params.base.value, params.base.decimals)?;

        match self {
            Self::Ethereum(chain) => {
                let fee_setting = pare_fee_setting(params.fee_setting.as_str())?;

                let balance = chain.balance(&params.base.from, None).await?;

                // check balance
                let remain_balance = ChainTransDomain::check_eth_balance(
                    &params.base.from,
                    balance,
                    params.base.token_address.as_deref(),
                    chain,
                    transfer_amount,
                )
                .await?;

                let fee = fee_setting.transaction_fee();
                // check transaction_fee
                if remain_balance < fee {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientFeeBalance,
                    ))?;
                }

                let params = chain::eth::operations::TransferOpt::try_from(&params.base)?;
                let tx_hash = chain
                    .exec_transaction(params, fee_setting, private_key)
                    .await?;

                Ok(TransferResp::new(
                    tx_hash,
                    unit::format_to_string(fee, eth::consts::ETH_DECIMAL)?,
                ))
            }
            Self::BitCoin(chain) => {
                let account = domain::chain::transaction::ChainTransDomain::account(
                    &params.base.chain_code,
                    &params.base.from,
                )
                .await?;
                let params = btc::operations::transfer::TransferArg::new(
                    &params.base.from,
                    &params.base.to,
                    &params.base.value,
                    account.address_type(),
                    chain.network,
                )?
                .with_spend_all(params.base.spend_all);

                let tx = chain
                    .transfer(params, private_key)
                    .await
                    .map_err(ChainTransDomain::handle_btc_fee_error)?;

                Ok(TransferResp::new(tx.tx_hash, tx.fee.to_string()))
            }
            Self::Ltc(chain) => {
                let account =
                    ChainTransDomain::account(&params.base.chain_code, &params.base.from).await?;

                let address_type = LtcAddressType::try_from(account.address_type())?;

                let params = ltc::operations::transfer::TransferArg::new(
                    &params.base.from,
                    &params.base.to,
                    &params.base.value,
                    address_type,
                    chain.network,
                )?
                .with_spend_all(params.base.spend_all);

                let tx = chain
                    .transfer(params, private_key)
                    .await
                    .map_err(ChainTransDomain::handle_btc_fee_error)?;

                Ok(TransferResp::new(tx.tx_hash, tx.fee.to_string()))
            }
            Self::Doge(chain) => {
                let account =
                    ChainTransDomain::account(&params.base.chain_code, &params.base.from).await?;

                let address_type = DogAddressType::try_from(account.address_type())?;

                let params = dog::operations::transfer::TransferArg::new(
                    &params.base.from,
                    &params.base.to,
                    &params.base.value,
                    address_type,
                    chain.network,
                )?
                .with_spend_all(params.base.spend_all);

                let tx = chain
                    .transfer(params, private_key)
                    .await
                    .map_err(ChainTransDomain::handle_btc_fee_error)?;

                Ok(TransferResp::new(tx.tx_hash, tx.fee.to_string()))
            }
            Self::Solana(chain) => {
                // check balance
                let balance = chain.balance(&params.base.from, None).await?;
                let remain_balance = ChainTransDomain::check_sol_balance(
                    &params.base.from,
                    balance,
                    params.base.token_address.as_deref(),
                    chain,
                    transfer_amount,
                )
                .await?;

                let token = params.base.token_address.clone();
                let params = sol::operations::transfer::TransferOpt::new(
                    &params.base.from,
                    &params.base.to,
                    &params.base.value,
                    params.base.token_address.clone(),
                    params.base.decimals,
                    chain.get_provider(),
                )?;

                let instructions = params.instructions().await?;
                let mut fee_setting = chain.estimate_fee_v1(&instructions, &params).await?;
                ChainTransDomain::sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

                ChainTransDomain::check_sol_transaction_fee(
                    remain_balance,
                    fee_setting.original_fee(),
                )?;
                let fee = fee_setting.transaction_fee().to_string();

                let tx_hash = chain
                    .exec_transaction(params, private_key, Some(fee_setting), instructions, 0)
                    .await?;

                Ok(TransferResp::new(tx_hash, fee))
            }
            Self::Tron(chain) => {
                if let Some(contract) = &params.base.token_address {
                    let mut transfer_params = tron::operations::transfer::ContractTransferOpt::new(
                        contract,
                        &params.base.from,
                        &params.base.to,
                        transfer_amount,
                        params.base.notes.clone(),
                    )?;

                    if let Some(signer) = &params.signer {
                        transfer_params = transfer_params.with_permission(signer.permission_id);
                    }

                    let provider = chain.get_provider();
                    let balance = chain
                        .balance(&params.base.from, Some(contract.clone()))
                        .await?;
                    if balance < transfer_amount {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientBalance,
                        ))?;
                    }

                    let account = provider
                        .account_info(&transfer_params.owner_address)
                        .await?;
                    // 主币是否有钱(可能账号未被初始化)
                    if account.balance <= 0 {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }

                    // constant contract to fee
                    let constant = transfer_params
                        .constant_contract(chain.get_provider())
                        .await?;
                    let consumer = provider
                        .contract_fee(constant, 1, &transfer_params.owner_address)
                        .await?;

                    if account.balance < consumer.transaction_fee_i64() {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }

                    // 需要实际消耗的资源
                    let net_used = consumer.act_bandwidth() as u64;
                    let energy_used = consumer.act_energy() as u64;

                    let fee = consumer.transaction_fee();
                    transfer_params.set_fee_limit(consumer);

                    let bill_consumer = BillResourceConsume::new_tron(net_used, energy_used);
                    let tx_hash = chain.exec_transaction(transfer_params, private_key).await?;

                    let mut resp = TransferResp::new(tx_hash, fee);
                    resp.with_consumer(bill_consumer);

                    Ok(resp)
                } else {
                    // 转账的金额转换为sun
                    let value_f64 = unit::format_to_f64(transfer_amount, params.base.decimals)?;
                    let value_i64 = (value_f64 * tron::consts::TRX_TO_SUN as f64) as i64;

                    let mut param = tron::operations::transfer::TransferOpt::new(
                        &params.base.from,
                        &params.base.to,
                        transfer_amount,
                        params.base.notes.clone(),
                    )?;
                    if let Some(signer) = &params.signer {
                        param = param.with_permission(signer.permission_id);
                    }
                    let provider = chain.get_provider();
                    let account = provider.account_info(&param.from).await?;
                    if account.balance <= 0 {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientBalance,
                        ))?;
                    }

                    let tx = param.build_raw_transaction(provider).await?;
                    let consumer = provider
                        .transfer_fee(&param.from, Some(&param.to), &tx.raw_data_hex, 1)
                        .await?;

                    if account.balance < consumer.transaction_fee_i64() + value_i64 {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientBalance,
                        ))?;
                    }

                    let bill_consumer =
                        BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0);

                    let tx_hash = chain.exec_transaction_v1(tx, private_key).await?;

                    let mut resp = TransferResp::new(tx_hash, consumer.transaction_fee());
                    resp.with_consumer(bill_consumer);

                    Ok(resp)
                }
            }
            Self::Ton(chain) => {
                // 验证余额
                let balance = chain
                    .balance(&params.base.from, params.base.token_address.clone())
                    .await?;
                if balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }

                let account =
                    ChainTransDomain::account(&params.base.chain_code, &params.base.from).await?;

                let address_type = match account.address_type() {
                    Some(ty) => TonAddressType::try_from(ty.as_str())?,
                    None => Err(crate::ServiceError::Types(
                        wallet_types::error::Error::MissAddressType,
                    ))?,
                };

                let msg_cell =
                    ton_tx::build_ext_cell(&params.base, &chain.provider, address_type).await?;

                let fee = chain
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
                    let balance = chain.balance(&params.base.from, None).await?;
                    if balance < trans_fee {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }
                }
                let tx_hash = chain.exec(msg_cell, private_key, address_type).await?;

                Ok(TransferResp::new(tx_hash, fee.get_fee_ton().to_string()))
            }
            Self::Sui(chain) => {
                let balance = chain
                    .balance(&params.base.from, params.base.token_address.clone())
                    .await?;
                if balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }

                let req = sui::transfer::TransferOpt::new(
                    &params.base.from,
                    &params.base.to,
                    transfer_amount,
                    params.base.token_address.clone(),
                )?;

                let mut helper = req.select_coin(&chain.provider).await?;
                let pt = req.build_pt(&chain.provider, &mut helper, None).await?;

                let gas = chain.estimate_fee(&params.base.from, pt).await?;

                let mut trans_fee = U256::from(gas.get_fee());
                if params.base.token_address.is_none() {
                    trans_fee += transfer_amount;
                    if balance < trans_fee {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }
                } else {
                    let balance = chain.balance(&params.base.from, None).await?;
                    if balance < trans_fee {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }
                }

                let fee = gas.get_fee_f64();
                let tx_data = req.build_data(&chain.provider, helper, gas).await?;
                let tx_hash = chain.exec(tx_data, private_key).await?;

                Ok(TransferResp::new(tx_hash, fee.to_string()))
            }
        }
    }

    pub async fn estimate_fee(
        &self,
        req: transaction::BaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency = domain::coin::token_price::TokenCurrencyGetter::get_currency(
            currency,
            &req.chain_code,
            main_symbol,
        )
        .await?;

        let res = match self {
            Self::Ethereum(chain) => {
                let value = unit::convert_to_u256(&req.value, req.decimals)?;
                let balance = chain.balance(&req.from, req.token_address.clone()).await?;
                if balance < value {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }

                let gas_oracle =
                    ChainTransDomain::gas_oracle(&req.chain_code, &chain.provider, backend).await?;

                let params = eth::operations::TransferOpt::new(
                    &req.from,
                    &req.to,
                    value,
                    req.token_address,
                )?;
                let fee = chain.estimate_gas(params).await?;
                let fee = FeeDetails::try_from((gas_oracle, fee.consume))?
                    .to_resp(token_currency, currency);

                wallet_utils::serde_func::serde_to_string(&fee)
            }
            Self::BitCoin(chain) => {
                let account = ChainTransDomain::account(&req.chain_code, &req.from).await?;
                let params = btc::operations::transfer::TransferArg::new(
                    &req.from,
                    &req.to,
                    &req.value,
                    account.address_type(),
                    chain.network,
                )?
                .with_spend_all(req.spend_all);

                let fee = chain
                    .estimate_fee(params, None)
                    .await
                    .map_err(domain::chain::transaction::ChainTransDomain::handle_btc_fee_error)?;

                let res = response_vo::CommonFeeDetails::new(
                    fee.transaction_fee_f64(),
                    token_currency,
                    currency,
                );
                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Ltc(chain) => {
                let account = ChainTransDomain::account(&req.chain_code, &req.from).await?;

                let address_type = LtcAddressType::try_from(account.address_type())?;

                let params = ltc::operations::transfer::TransferArg::new(
                    &req.from,
                    &req.to,
                    &req.value,
                    address_type,
                    chain.network,
                )?
                .with_spend_all(req.spend_all);

                let fee = chain
                    .estimate_fee(params)
                    .await
                    .map_err(domain::chain::transaction::ChainTransDomain::handle_btc_fee_error)?;

                let res = response_vo::CommonFeeDetails::new(
                    fee.transaction_fee_f64(),
                    token_currency,
                    currency,
                );
                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Doge(chain) => {
                let account = ChainTransDomain::account(&req.chain_code, &req.from).await?;

                let address_type = DogAddressType::try_from(account.address_type())?;

                let params = dog::operations::transfer::TransferArg::new(
                    &req.from,
                    &req.to,
                    &req.value,
                    address_type,
                    chain.network,
                )?
                .with_spend_all(req.spend_all);

                let fee = chain
                    .estimate_fee(params)
                    .await
                    .map_err(domain::chain::transaction::ChainTransDomain::handle_btc_fee_error)?;

                let res = response_vo::CommonFeeDetails::new(
                    fee.transaction_fee_f64(),
                    token_currency,
                    currency,
                );
                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Solana(chain) => {
                let token = req.token_address.clone();
                let params = sol::operations::transfer::TransferOpt::new(
                    &req.from,
                    &req.to,
                    &req.value,
                    req.token_address,
                    req.decimals,
                    chain.get_provider(),
                )?;

                let instructions = params.instructions().await?;
                let mut fee_setting = chain.estimate_fee_v1(&instructions, &params).await?;

                ChainTransDomain::sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

                let res = response_vo::CommonFeeDetails::new(
                    fee_setting.transaction_fee(),
                    token_currency,
                    currency,
                );
                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Tron(chain) => {
                let value = unit::convert_to_u256(&req.value, req.decimals)?;

                let consumer = if let Some(contract) = req.token_address {
                    let balance = chain.balance(&req.from, Some(contract.clone())).await?;
                    if balance < value {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientBalance,
                        ))?;
                    }

                    let params = tron::operations::transfer::ContractTransferOpt::new(
                        &contract,
                        &req.from,
                        &req.to,
                        value,
                        req.notes.clone(),
                    )?;

                    chain.contract_fee(&req.from, 1, params).await?
                } else {
                    let params = tron::operations::transfer::TransferOpt::new(
                        &req.from,
                        &req.to,
                        value,
                        req.notes.clone(),
                    )?;

                    chain
                        .simulate_simple_fee(&req.from, &req.to, 1, params)
                        .await?
                };
                let token_currency = domain::coin::token_price::TokenCurrencyGetter::get_currency(
                    currency,
                    &req.chain_code,
                    main_symbol,
                )
                .await?;

                let res = TronFeeDetails::new(consumer, token_currency, currency)?;
                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Ton(chain) => {
                let account = ChainTransDomain::account(&req.chain_code, &req.from).await?;

                let address_type = match account.address_type() {
                    Some(ty) => TonAddressType::try_from(ty.as_str())?,
                    None => Err(crate::ServiceError::Types(
                        wallet_types::error::Error::MissAddressType,
                    ))?,
                };

                let msg_cell = ton_tx::build_ext_cell(&req, &chain.provider, address_type).await?;

                let fee = chain
                    .estimate_fee(msg_cell.clone(), &req.from, address_type)
                    .await?;

                let res =
                    response_vo::CommonFeeDetails::new(fee.get_fee_ton(), token_currency, currency);

                wallet_utils::serde_func::serde_to_string(&res)
            }
            Self::Sui(chain) => {
                let amount = unit::convert_to_u256(&req.value, req.decimals)?;
                let params = sui::transfer::TransferOpt::new(
                    &req.from,
                    &req.to,
                    amount,
                    req.token_address.clone(),
                )?;

                let mut helper = params.select_coin(&chain.provider).await?;
                let pt = params.build_pt(&chain.provider, &mut helper, None).await?;

                let gas = chain.estimate_fee(&req.from, pt).await?;

                let res =
                    response_vo::CommonFeeDetails::new(gas.get_fee_f64(), token_currency, currency);

                wallet_utils::serde_func::serde_to_string(&res)
            }
        };
        Ok(res?)
    }

    pub async fn approve(
        &self,
        req: &transaction::ApproveReq,
        key: ChainPrivateKey,
        value: alloy::primitives::U256,
    ) -> Result<String, crate::ServiceError> {
        let hash = match self {
            Self::Ethereum(chain) => eth_tx::approve(chain, req, value, key).await?,
            Self::Tron(chain) => tron_tx::approve(chain, req, value, key).await?,
            _ => {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::NotSupportChain,
                ))?
            }
        };

        Ok(hash)
    }

    pub async fn approve_fee(
        &self,
        req: &transaction::ApproveReq,
        value: alloy::primitives::U256,
        main_symbol: &str,
    ) -> Result<String, crate::ServiceError> {
        let currency = {
            let currency = crate::app_state::APP_STATE.read().await;
            currency.currency().to_string()
        };

        let token_currency = domain::coin::token_price::TokenCurrencyGetter::get_currency(
            &currency,
            &req.chain_code,
            main_symbol,
        )
        .await?;

        let fee = match self {
            Self::Ethereum(chain) => {
                let fee = eth_tx::approve_fee(chain, req, value).await?;

                let backend = crate::manager::Context::get_global_backend_api()?;

                // 使用默认的手续费配置
                let gas_oracle =
                    ChainTransDomain::gas_oracle(&req.chain_code, &chain.provider, backend).await?;

                let fee = FeeDetails::try_from((gas_oracle, fee.consume))?
                    .to_resp(token_currency, &currency);

                wallet_utils::serde_func::serde_to_string(&fee)?
            }
            Self::Tron(chain) => {
                let consumer = tron_tx::approve_fee(chain, req, value).await?;

                let res = TronFeeDetails::new(consumer, token_currency, &currency)?;
                wallet_utils::serde_func::serde_to_string(&res)?
            }
            _ => {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::NotSupportChain,
                ))?
            }
        };

        Ok(fee)
    }

    // pub async fn deposit(
    //     &self,
    //     req: &transaction::DepositParams,
    //     decimals: u8,
    //     key: ChainPrivateKey,
    // ) -> Result<String, crate::ServiceError> {
    //     let value = wallet_utils::unit::convert_to_u256(&req.value, decimals)?;

    //     let hash = match self {
    //         Self::Ethereum(chain) => eth_tx::deposit(chain, req, value, key).await?,
    //         _ => {
    //             return Err(crate::BusinessError::Chain(
    //                 crate::ChainError::NotSupportChain,
    //             ))?
    //         }
    //     };

    //     Ok(hash)
    // }

    pub async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<U256, crate::ServiceError> {
        let resp = match self {
            Self::Ethereum(chain) => eth_tx::allowance(chain, from, token, spender).await?,
            Self::Tron(chain) => tron_tx::allowance(chain, from, token, spender).await?,
            _ => {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::NotSupportChain,
                ))?
            }
        };

        Ok(resp)
    }

    pub async fn swap_quote(
        &self,
        req: &QuoteReq,
        quote_resp: &AggQuoteResp,
    ) -> Result<EstimateSwapResult, crate::ServiceError> {
        // note:如果token_in 是主币，则传入0地址

        let amount_out = quote_resp.amount_out_u256(req.token_out.decimals as u8)?;
        let min_amount_out =
            calc_slippage(amount_out, req.get_slippage(quote_resp.default_slippage));

        let resp = match self {
            Self::Ethereum(chain) => {
                let token_in = if req.token_in.token_addr.is_empty() {
                    alloy::primitives::Address::ZERO
                } else {
                    wallet_utils::address::parse_eth_address(&req.recipient)?
                };

                let swap_params = SwapParams {
                    aggregator_addr: req.aggregator_address()?,
                    amount_in: req.amount_in_u256()?,
                    min_amount_out,
                    recipient: wallet_utils::address::parse_eth_address(&req.recipient)?,
                    token_in,
                    token_out: wallet_utils::address::parse_eth_address(&req.token_out.token_addr)?,
                    dex_router: quote_resp.dex_route_list.clone(),
                    allow_partial_fill: req.allow_partial_fill,
                };

                eth_tx::estimate_swap(swap_params, chain).await?
            }
            Self::Tron(chain) => {
                let token_in = if req.token_in.token_addr.is_empty() {
                    alloy::primitives::Address::ZERO
                } else {
                    QuoteReq::addr_tron_to_eth(&req.token_in.token_addr)?
                };

                let swap_params = SwapParams {
                    aggregator_addr: QuoteReq::addr_tron_to_eth(&req.aggregator_addr)?,
                    amount_in: req.amount_in_u256()?,
                    min_amount_out,
                    recipient: QuoteReq::addr_tron_to_eth(&req.recipient)?,
                    token_in,
                    token_out: QuoteReq::addr_tron_to_eth(&req.token_out.token_addr)?,
                    dex_router: quote_resp.dex_route_list.clone(),
                    allow_partial_fill: req.allow_partial_fill,
                };

                tron_tx::estimate_swap(&swap_params, chain).await?
            }
            _ => {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::NotSupportChain,
                ))?
            }
        };

        Ok(resp)
    }

    pub async fn swap(
        &self,
        req: &SwapReq,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<String, crate::ServiceError> {
        let swap_params = SwapParams::try_from(req)?;

        let resp = match self {
            Self::Ethereum(chain) => eth_tx::swap(chain, &swap_params, fee, key).await?,
            Self::Tron(chain) => tron_tx::swap(chain, &swap_params, key).await?,
            _ => {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::NotSupportChain,
                ))?
            }
        };

        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{chain::adapter::ChainAdapterFactory, swap_client::AggQuoteResp},
        request::transaction::{DexRoute, QuoteReq, RouteInDex, SwapTokenInfo},
    };
    use wallet_utils::{init_test_log, unit};

    #[tokio::test]
    async fn test_estimate_swap() {
        init_test_log();

        let chain_code = "tron";
        // let rpc_url = "http://127.0.0.1:8545";
        let rpc_url = "http://100.78.188.103:8090";

        let adapter = ChainAdapterFactory::get_node_transaction_adapter(chain_code, rpc_url)
            .await
            .unwrap();

        let amount_in = unit::convert_to_u256("0.1", 6).unwrap();

        // 模拟聚合器的响应
        let resp = AggQuoteResp {
            chain_code: "tron".to_string(),
            amount_in: amount_in.to_string(),
            amount_out: "0".to_string(),
            dex_route_list: vec![DexRoute {
                amount_in: amount_in.to_string(),
                amount_out: "0".to_string(),
                route_in_dex: vec![
                    RouteInDex {
                        dex_id: 3,
                        pool_id: "TSUUVjysXV8YqHytSNjfkNXnnB49QDvZpx".to_string(),
                        zero_for_one: true,
                        amount_in: amount_in.to_string(),
                        min_amount_out: "0".to_string(),
                        in_token_addr: "TNUC9Qb1rRpS5CbWLmNMxXBjyFoydXjWFR".to_string(),
                        out_token_addr: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
                        fee: "0".to_string(),
                    },
                    // RouteInDex {
                    //     dex_id: 2,
                    //     pool_id: "0x3041CbD36888bECc7bbCBc0045E3B1f144466f5f".to_string(),
                    //     zero_for_one: true,
                    //     amount_in: "0".to_string(),
                    //     min_amount_out: "0".to_string(),
                    //     in_token_addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
                    //     out_token_addr: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
                    //     fee: "0".to_string(),
                    // },
                ],
            }],
            default_slippage: 2,
        };

        let token_in = SwapTokenInfo {
            token_addr: "".to_string(),
            symbol: "TRX".to_string(),
            decimals: 6,
        };

        let token_out = SwapTokenInfo {
            token_addr: "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_string(),
            symbol: "USDT".to_string(),
            decimals: 6,
        };

        let req = QuoteReq {
            aggregator_addr: "TS7x5pq98ZjHPBKM2NEchvJWnevM7RJb4E".to_string(),
            recipient: "TMrVocuPpNqf3fpPSSWy7V8kyAers3p1Jc".to_string(),
            chain_code: "tron".to_string(),
            amount_in: "0.1".to_string(),
            token_in,
            token_out,
            dex_list: vec![2, 3],
            slippage: Some(0.2),
            allow_partial_fill: false,
        };

        let result = adapter.swap_quote(&req, &resp).await.unwrap();

        tracing::warn!(
            "amount_in {}",
            unit::format_to_f64(result.amount_in, req.token_in.decimals as u8).unwrap(),
        );

        tracing::warn!(
            "amount_out {}",
            unit::format_to_f64(result.amount_out, req.token_out.decimals as u8).unwrap(),
        );
    }
}
