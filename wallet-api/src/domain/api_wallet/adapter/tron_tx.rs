use crate::{
    domain::{
        api_wallet::adapter::{Multisig, Tx, TIME_OUT},
        chain::{
            swap::{
                calc_slippage, evm_swap::{dexSwap1Call, SwapParams},
                EstimateSwapResult,
            },
            TransferResp,
        },
        coin::TokenCurrencyGetter,
        multisig::MultisigQueueDomain,
    },
    infrastructure::swap_client::AggQuoteResp,
    request::transaction::{
        ApproveReq, BaseTransferReq, DepositReq, QuoteReq, SwapReq, TransferReq, WithdrawReq,
    },
    response_vo::{MultisigQueueFeeParams, TransferParams, TronFeeDetails},
    ServiceError,
};
use alloy::{
    primitives::U256,
    sol_types::{SolCall, SolValue},
};
use std::collections::HashMap;
use wallet_chain_interact::{
    abi_encode_u256, tron, tron::{
        operations::{
            contract::{TriggerContractParameter, WarpContract}, multisig::TransactionOpt,
            transfer::{ContractTransferOpt, TransferOpt},
            trc::{Allowance, Approve, Deposit},
            TronConstantOperation as _,
            TronTxOperation,
        },
        params::ResourceConsumer,
        protocol::account::AccountResourceDetail,
        TronChain,
    }, types::{ChainPrivateKey, FetchMultisigAddressResp, MultisigSignResp, MultisigTxResp}, BillResourceConsume,
    Error,
    QueryTransactionResult,
};
use wallet_database::entities::{
    api_assets::ApiAssetsEntity, coin::CoinEntity,
    multisig_account::MultisigAccountEntity, multisig_member::MultisigMemberEntities,
    multisig_queue::MultisigQueueEntity, permission::PermissionEntity,
};
use wallet_transport::client::HttpClient;
use wallet_transport_backend::api::BackendApi;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::unit;
use crate::request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq};

pub(crate) struct TronTx {
    chain: TronChain,
}

impl TronTx {
    pub fn new(
        rpc_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, wallet_chain_interact::Error> {
        let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let http_client = HttpClient::new(rpc_url, header_opt, timeout)?;
        let provider = tron::Provider::new(http_client)?;

        let tron_chain = TronChain::new(provider)?;
        Ok(Self { chain: tron_chain })
    }

    // 构建多签交易
    pub(super) async fn build_build_tx(
        &self,
        req: &TransferParams,
        token: Option<String>,
        value: U256,
        threshold: i64,
        permission_id: Option<i64>,
    ) -> Result<MultisigTxResp, crate::ServiceError> {
        let expiration = MultisigQueueDomain::sub_expiration(req.expiration.unwrap_or(1));

        if let Some(token) = token {
            let mut params =
                ContractTransferOpt::new(&token, &req.from, &req.to, value, req.notes.clone())?;

            params.permission_id = permission_id;

            let provider = self.chain.get_provider();
            let constant = params.constant_contract(provider).await?;
            let consumer = provider.contract_fee(constant, threshold as u8, &req.from).await?;
            params.set_fee_limit(consumer);

            Ok(self.chain.build_multisig_transaction(params, expiration as u64).await?)
        } else {
            let mut params = TransferOpt::new(&req.from, &req.to, value, req.notes.clone())?;
            params.permission_id = permission_id;

            Ok(self.chain.build_multisig_transaction(params, expiration as u64).await?)
        }
    }

    pub(super) async fn estimate_swap(
        &self,
        swap_params: &SwapParams,
    ) -> Result<EstimateSwapResult<ResourceConsumer>, crate::ServiceError> {
        let (params, owner_address) = self.build_base_swap(swap_params)?;

        let wrap = WarpContract { params };

        // 模拟交易结果
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        constant.is_success()?;

        let bytes = wallet_utils::hex_func::hex_decode(&constant.constant_result[0])?;

        // 模拟的结果k
        let (amount_in, amount_out): (U256, U256) =
            <(U256, U256)>::abi_decode_params(&bytes, true).map_err(|e| {
                crate::ServiceError::AggregatorError { code: -1, agg_code: 0, msg: e.to_string() }
            })?;

        // get fee
        let mut consumer = self.chain.provider.contract_fee(constant, 1, &owner_address).await?;
        // 手续费增加0.2trx
        consumer.set_extra_fee(200000);

        let resp = EstimateSwapResult { amount_in, amount_out, consumer };

        Ok(resp)
    }

    fn build_base_swap(
        &self,
        swap_params: &SwapParams,
    ) -> Result<(TriggerContractParameter, String), crate::ServiceError> {
        let call_value = dexSwap1Call::try_from((swap_params, ChainCode::Tron))?;

        // tracing::warn!("call value: {:#?}", call_value);
        let contract_address = swap_params.aggregator_tron_addr()?;
        let owner_address = swap_params.recipient_tron_addr()?;

        let contract_address = wallet_utils::address::bs58_addr_to_hex(&contract_address)?;
        let owner_address = wallet_utils::address::bs58_addr_to_hex(&owner_address)?;

        let mut raw = vec![];
        call_value.abi_encode_raw(&mut raw);

        // 构建调用合约的参数
        let parameter = wallet_utils::hex_func::hex_encode(raw);
        let function_selector = "dexSwap1(((uint16,address,bool,uint256,uint256)[])[],address,address,uint256,uint256,address,bool)";
        let mut value = TriggerContractParameter::new(
            &contract_address,
            &owner_address,
            &function_selector,
            parameter,
        );

        // 主币设置转账的value
        if swap_params.token_in.is_zero() {
            value.call_value = Some(swap_params.amount_in.to::<u64>());
        }

        Ok((value, owner_address))
    }

    fn build_base_withdraw(
        &self,
        req: &WithdrawReq,
        value: U256,
    ) -> Result<TriggerContractParameter, crate::ServiceError> {
        // 构建调用合约的参数
        let parameter = abi_encode_u256(value);
        let function_selector = "withdraw(uint256)";

        let contract_address = wallet_utils::address::bs58_addr_to_hex(&req.token)?;
        let owner_address = wallet_utils::address::bs58_addr_to_hex(&req.from)?;

        let value = TriggerContractParameter::new(
            &contract_address,
            &owner_address,
            &function_selector,
            parameter,
        );

        Ok(value)
    }
}

#[async_trait::async_trait]
impl Tx for TronTx {
    async fn account_resource(
        &self,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, ServiceError> {
        let resource = self.chain.account_resource(owner_address).await?;
        Ok(resource)
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
        let transfer_amount = self.check_min_transfer(&params.base.value, params.base.decimals)?;
        if let Some(contract) = &params.base.token_address {
            let mut transfer_params = ContractTransferOpt::new(
                contract,
                &params.base.from,
                &params.base.to,
                transfer_amount,
                params.base.notes.clone(),
            )?;

            // if let Some(signer) = &params.signer {
            //     transfer_params = transfer_params.with_permission(signer.permission_id);
            // }

            let provider = self.chain.get_provider();
            let balance = self.chain.balance(&params.base.from, Some(contract.clone())).await?;
            if balance < transfer_amount {
                return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
            }

            let account = provider.account_info(&transfer_params.owner_address).await?;
            // 主币是否有钱(可能账号未被初始化)
            if account.balance <= 0 {
                return Err(crate::BusinessError::Chain(
                    crate::ChainError::InsufficientFeeBalance,
                ))?;
            }

            // constant contract to fee
            let constant = transfer_params.constant_contract(self.chain.get_provider()).await?;
            let consumer =
                provider.contract_fee(constant, 1, &transfer_params.owner_address).await?;

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
            let tx_hash = self.chain.exec_transaction(transfer_params, private_key).await?;

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
            // if let Some(signer) = &params.signer {
            //     param = param.with_permission(signer.permission_id);
            // }
            let provider = self.chain.get_provider();
            let account = provider.account_info(&param.from).await?;
            if account.balance <= 0 {
                return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
            }

            let tx = param.build_raw_transaction(provider).await?;
            let consumer =
                provider.transfer_fee(&param.from, Some(&param.to), &tx.raw_data_hex, 1).await?;

            if account.balance < consumer.transaction_fee_i64() + value_i64 {
                return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
            }

            let bill_consumer = BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0);

            let tx_hash = self.chain.exec_transaction_v1(tx, private_key).await?;

            let mut resp = TransferResp::new(tx_hash, consumer.transaction_fee());
            resp.with_consumer(bill_consumer);

            Ok(resp)
        }
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
        let consumer = if let Some(contract) = req.token_address {
            let balance = self.chain.balance(&req.from, Some(contract.clone())).await?;
            if balance < value {
                return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
            }

            let params = tron::operations::transfer::ContractTransferOpt::new(
                &contract,
                &req.from,
                &req.to,
                value,
                req.notes.clone(),
            )?;

            self.chain.contract_fee(&req.from, 1, params).await?
        } else {
            let params = tron::operations::transfer::TransferOpt::new(
                &req.from,
                &req.to,
                value,
                req.notes.clone(),
            )?;

            self.chain.simulate_simple_fee(&req.from, &req.to, 1, params).await?
        };
        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &req.chain_code, main_symbol, None).await?;

        let res = TronFeeDetails::new(consumer, token_currency, currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;
        Ok(fee)
    }

    async fn approve(
        &self,
        req: &ApproveReq,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let approve = Approve::new(&req.from, &req.spender, &req.contract, value);
        let mut wrap = WarpContract::new(approve)?;

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let consumer = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        // check balance
        let balance = self.chain.balance(&req.from, None).await?;
        let fee = alloy::primitives::U256::from(consumer.transaction_fee_i64());
        if balance < fee {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance))?;
        }

        // get consumer
        let bill_consumer = BillResourceConsume::new_tron(
            consumer.act_bandwidth() as u64,
            consumer.act_energy() as u64,
        );

        // exec trans
        let raw_transaction = wrap.trigger_smart_contract(&self.chain.provider, &consumer).await?;
        let result = self.chain.exec_transaction_v1(raw_transaction, key).await?;

        let mut resp = TransferResp::new(result, consumer.transaction_fee());
        resp.with_consumer(bill_consumer);

        Ok(resp)
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

        let approve = Approve::new(&req.from, &req.spender, &req.contract, value);
        let wrap = WarpContract::new(approve)?;

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let consumer = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        let res = TronFeeDetails::new(consumer, token_currency, &currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok(fee)
    }

    async fn allowance(
        &self,
        from: &str,
        token: &str,
        spender: &str,
    ) -> Result<U256, ServiceError> {
        let approve = Allowance::new(from, spender, token);
        let wrap = WarpContract::new(approve)?;

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;

        Ok(constant.parse_u256()?)
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
            aggregator_addr: QuoteReq::addr_tron_to_eth(&req.aggregator_addr)?,
            amount_in: req.amount_in_u256()?,
            min_amount_out,
            recipient: QuoteReq::addr_tron_to_eth(&req.recipient)?,
            token_in: SwapParams::tron_parse_or_zero_addr(&req.token_in.token_addr)?,
            token_out: SwapParams::tron_parse_or_zero_addr(&req.token_out.token_addr)?,
            dex_router: quote_resp.dex_route_list.clone(),
            allow_partial_fill: req.allow_partial_fill,
        };

        let resp = self.estimate_swap(&swap_params).await?;

        let consumer = wallet_utils::serde_func::serde_to_string(&resp.consumer)?;

        let res = TronFeeDetails::new(resp.consumer, token_currency, &currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok((resp.amount_out, consumer, fee))
    }

    async fn swap(
        &self,
        req: &SwapReq,
        fee: String,
        key: ChainPrivateKey,
    ) -> Result<TransferResp, ServiceError> {
        let swap_params = SwapParams::try_from(req)?;
        let (params, owner_address) = self.build_base_swap(&swap_params)?;

        let mut wrap = WarpContract { params };
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        // get fee
        let mut consumer = self.chain.provider.contract_fee(constant, 1, &owner_address).await?;

        // check fee
        let balance = self.chain.balance(&swap_params.recipient_tron_addr()?, None).await?;
        // 手续费增加0.2trx
        consumer.set_extra_fee(200000);

        let mut fee = alloy::primitives::U256::from(consumer.transaction_fee_i64());
        if swap_params.main_coin_swap() {
            fee += swap_params.amount_in;
        }
        if balance < fee {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance))?;
        }

        let bill_consumer = BillResourceConsume::new_tron(
            consumer.act_bandwidth() as u64,
            consumer.act_energy() as u64,
        );

        let raw_transaction = wrap.trigger_with_fee(&self.chain.provider, 300).await?;

        let tx_hash = self.chain.exec_transaction_v1(raw_transaction, key).await?;

        let mut resp = TransferResp::new(tx_hash, consumer.transaction_fee());
        resp.with_consumer(bill_consumer);

        Ok(resp)
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

        let approve = Deposit::new(&req.from, &req.token, value);
        let wrap = WarpContract::new(approve)?;

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let resource = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        let consumer = wallet_utils::serde_func::serde_to_string(&resource)?;

        let res = TronFeeDetails::new(resource, token_currency, &currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok((consumer, fee))
    }

    async fn deposit(
        &self,
        req: &DepositReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let approve = Deposit::new(&req.from, &req.token, value);
        let mut wrap = WarpContract::new(approve)?;

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let consumer = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        // check balance
        let balance = self.chain.balance(&req.from, None).await?;
        let fee = alloy::primitives::U256::from(consumer.transaction_fee_i64()) + value;
        if balance < fee {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance))?;
        }

        // get consumer
        let bill_consumer = BillResourceConsume::new_tron(
            consumer.act_bandwidth() as u64,
            consumer.act_energy() as u64,
        );

        // exec trans
        let raw_transaction = wrap.trigger_smart_contract(&self.chain.provider, &consumer).await?;
        let result = self.chain.exec_transaction_v1(raw_transaction, key).await?;

        let mut resp = TransferResp::new(result, consumer.transaction_fee());
        resp.with_consumer(bill_consumer);

        Ok(resp)
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

        let trigger = self.build_base_withdraw(&req, value)?;
        let wrap = WarpContract { params: trigger };

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let resource = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        let consumer = wallet_utils::serde_func::serde_to_string(&resource)?;

        let res = TronFeeDetails::new(resource, token_currency, &currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok((consumer, fee))
    }

    async fn withdraw(
        &self,
        req: &WithdrawReq,
        fee: String,
        key: ChainPrivateKey,
        value: U256,
    ) -> Result<TransferResp, ServiceError> {
        let trigger = self.build_base_withdraw(req, value)?;
        let mut wrap = WarpContract { params: trigger };

        // get fee
        let constant = wrap.trigger_constant_contract(&self.chain.provider).await?;
        let consumer = self.chain.provider.contract_fee(constant, 1, &req.from).await?;

        let balance = self.chain.balance(&req.from, None).await?;
        let fee = alloy::primitives::U256::from(consumer.transaction_fee_i64());
        if balance < fee {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance))?;
        }

        // get consumer
        let bill_consumer = BillResourceConsume::new_tron(
            consumer.act_bandwidth() as u64,
            consumer.act_energy() as u64,
        );

        // exec trans
        let raw_transaction = wrap.trigger_smart_contract(&self.chain.provider, &consumer).await?;
        let result = self.chain.exec_transaction_v1(raw_transaction, key).await?;

        let mut resp = TransferResp::new(result, consumer.transaction_fee());
        resp.with_consumer(bill_consumer);

        Ok(resp)
    }
}

#[async_trait::async_trait]
impl Multisig for TronTx {
    async fn multisig_address(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
    ) -> Result<FetchMultisigAddressResp, ServiceError> {
        Ok(FetchMultisigAddressResp {
            authority_address: "".to_string(),
            multisig_address: account.address.to_string(),
            salt: "".to_string(),
        })
    }

    async fn deploy_multisig_account(
        &self,
        account: &MultisigAccountEntity,
        member: &MultisigMemberEntities,
        fee_setting: Option<String>,
        key: ChainPrivateKey,
    ) -> Result<(String, String), ServiceError> {
        let params = tron::operations::multisig::MultisigAccountOpt::new(
            &account.initiator_addr,
            account.threshold as u8,
            member.get_owner_str_vec(),
        )?;

        // check balance
        let provider = self.chain.get_provider();
        let tx = params.build_raw_transaction(provider).await?;
        let mut consumer =
            provider.transfer_fee(&account.initiator_addr, None, &tx.raw_data_hex, 1).await?;

        let chain_parameter = self.chain.provider.chain_params().await?;
        consumer.set_extra_fee(chain_parameter.update_account_fee());

        let fee = consumer.transaction_fee_i64();
        let account = provider.account_info(&account.initiator_addr).await?;
        if account.balance < fee {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
        }

        let consumer = BillResourceConsume::new_tron(consumer.bandwidth.consumer as u64, 0);
        let tx_hash = self.chain.exec_transaction_v1(tx, key).await?;

        Ok((tx_hash, consumer.to_json_str()?))
    }

    async fn deploy_multisig_fee(
        &self,
        account: &MultisigAccountEntity,
        member: MultisigMemberEntities,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency_lock = crate::app_state::APP_STATE.read().await;
        let currency = currency_lock.currency();

        let backend = crate::manager::Context::get_global_backend_api()?;

        let account_info = self.chain.get_provider().account_info(&account.initiator_addr).await?;
        if account_info.address.is_empty() {
            return Err(crate::BusinessError::Chain(crate::ChainError::AddressNotInit))?;
        }

        let params = tron::operations::multisig::MultisigAccountOpt::new(
            &account.initiator_addr,
            account.threshold as u8,
            member.get_owner_str_vec(),
        )?;
        let mut consumer = self.chain.simple_fee(&account.initiator_addr, 1, params).await?;

        let chain_parameter = self.chain.provider.chain_params().await?;
        consumer.set_extra_fee(chain_parameter.update_account_fee());

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &account.chain_code, main_symbol, None)
                .await?;

        let res = TronFeeDetails::new(consumer, token_currency, currency)?;
        Ok(wallet_utils::serde_func::serde_to_string(&res)?)
    }

    async fn build_multisig_fee(
        &self,
        req: &MultisigQueueFeeParams,
        account: &MultisigAccountEntity,
        decimal: u8,
        token: Option<String>,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok("".to_string())
    }

    async fn build_multisig_with_account(
        &self,
        req: &TransferParams,
        account: &MultisigAccountEntity,
        assets: &ApiAssetsEntity,
        key: ChainPrivateKey,
    ) -> Result<MultisigTxResp, ServiceError> {
        let decimal = assets.decimals;
        let token = assets.token_address();

        let value = self.check_min_transfer(&req.value, decimal)?;
        let balance = self.chain.balance(&req.from, token.clone()).await?;
        if balance < value {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
        }

        self.build_build_tx(req, token, value, account.threshold as i64, None).await
    }

    async fn build_multisig_with_permission(
        &self,
        req: &TransferParams,
        p: &PermissionEntity,
        coin: &CoinEntity,
    ) -> Result<MultisigTxResp, ServiceError> {
        let decimal = coin.decimals;
        let token = coin.token_address();

        let value = self.check_min_transfer(&req.value, decimal)?;
        let balance = self.chain.balance(&req.from, token.clone()).await?;
        if balance < value {
            return Err(crate::BusinessError::Chain(crate::ChainError::InsufficientBalance))?;
        }

        let permission_id = Some(p.active_id);
        self.build_build_tx(req, token, value, p.threshold, permission_id).await
    }

    async fn sign_fee(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        raw_data: &str,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        Ok(" ".to_string())
    }

    async fn sign_multisig_tx(
        &self,
        account: &MultisigAccountEntity,
        address: &str,
        key: ChainPrivateKey,
        raw_data: &str,
    ) -> Result<MultisigSignResp, ServiceError> {
        let res = TransactionOpt::sign_transaction(raw_data, key)?;
        Ok(res)
    }

    async fn estimate_multisig_fee(
        &self,
        queue: &MultisigQueueEntity,
        coin: &CoinEntity,
        backend: &BackendApi,
        sign_list: Vec<String>,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &queue.chain_code, main_symbol, None)
                .await?;

        let signature_num = sign_list.len() as u8;
        let value = unit::convert_to_u256(&queue.value, coin.decimals)?;
        let memo = (!queue.notes.is_empty()).then(|| queue.notes.clone());

        let mut consumer = if let Some(token) = coin.token_address() {
            let params = tron::operations::transfer::ContractTransferOpt::new(
                &token,
                &queue.from_addr,
                &queue.to_addr,
                value,
                memo,
            )?;

            self.chain.contract_fee(&queue.from_addr, signature_num, params).await?
        } else {
            let params =
                tron::operations::multisig::TransactionOpt::data_from_str(&queue.raw_data)?;

            let to = (!queue.to_addr.is_empty()).then_some(queue.to_addr.as_str());

            self.chain
                .provider
                .transfer_fee(&queue.from_addr, to, &params.raw_data_hex, signature_num)
                .await?
        };

        let token_currency =
            TokenCurrencyGetter::get_currency(currency, &queue.chain_code, main_symbol, None)
                .await?;

        // if queue.transfer_type == ApiBillKind::UpdatePermission.to_i8() {
        //     let chain = self.chain.provider.chain_params().await?;
        //     consumer.set_extra_fee(chain.update_account_fee());
        // }

        let res = TronFeeDetails::new(consumer, token_currency, currency)?;
        Ok(wallet_utils::serde_func::serde_to_string(&res)?)
    }
}
