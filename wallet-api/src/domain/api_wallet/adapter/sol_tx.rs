use crate::{
    domain::{
        api_wallet::adapter::{
            TIME_OUT,
            tx::{RawTx, Tx},
        },
        chain::{TransferResp, adapter::sol_tx::SYSTEM_ACCOUNT_RENT, transaction::DEFAULT_UNITS},
        coin::TokenCurrencyGetter,
    },
    error::{
        business::{BusinessError, chain::ChainError},
        service::ServiceError,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
    response_vo::CommonFeeDetails,
};
use alloy::primitives::U256;
use std::collections::HashMap;
use wallet_chain_interact::{
    Error, QueryTransactionResult,
    sol::{
        Provider, SolFeeSetting, SolanaChain,
        consts::SOL_DECIMAL,
        operations::{SolInstructionOperation, transfer::TransferOpt},
    },
    tron::protocol::account::AccountResourceDetail,
    types::ChainPrivateKey,
};
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_transport::client::RpcClient;

pub(crate) struct SolTx {
    chain: SolanaChain,
}

impl SolTx {
    pub fn new(
        rpc_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, wallet_chain_interact::Error> {
        // let network = wallet_types::chain::network::NetworkKind::Mainnet;
        let timeout = Some(std::time::Duration::from_secs(TIME_OUT));
        let rpc_client = RpcClient::new(rpc_url, header_opt, timeout)?;
        let provider = Provider::new(rpc_client)?;
        let sol_chain = SolanaChain::new(provider)?;
        Ok(Self { chain: sol_chain })
    }

    pub async fn check_sol_balance(
        &self,
        from: &str,
        balance: U256,
        token: Option<&str>,
        transfer_amount: U256,
    ) -> Result<U256, crate::error::service::ServiceError> {
        let cost_main = match token {
            Some(token) => {
                let token_balance = self.chain.balance(from, Some(token.to_string())).await?;
                if token_balance < transfer_amount {
                    return Err(crate::error::business::BusinessError::Chain(
                        crate::error::business::chain::ChainError::insufficient_balance_with_detail(
                            crate::error::business::chain::InsufficientBalanceDetail::new()
                                .from_addr(from.to_string())
                                .chain_code("sol")
                                .token_addr(token.to_string())
                                .value(transfer_amount.to_string())
                                .balance(token_balance.to_string())
                                .need(transfer_amount.to_string())
                                .reason("token balance is insufficient"),
                        ),
                    ))?;
                }
                balance
            }
            None => {
                if balance < transfer_amount {
                    return Err(crate::error::business::BusinessError::Chain(
                        crate::error::business::chain::ChainError::insufficient_balance_with_detail(
                            crate::error::business::chain::InsufficientBalanceDetail::new()
                                .from_addr(from.to_string())
                                .chain_code("sol")
                                .value(transfer_amount.to_string())
                                .balance(balance.to_string())
                                .need(transfer_amount.to_string())
                                .reason("main coin balance is insufficient"),
                        ),
                    ))?;
                }
                balance - transfer_amount
            }
        };
        Ok(cost_main)
    }

    pub fn sol_priority_fee(
        &self,
        fee_setting: &mut SolFeeSetting,
        token: Option<&String>,
        units: u64,
    ) {
        if let Some(_token) = token {
            fee_setting.compute_units_consumed = units;
            fee_setting.priority_fee_per_compute_unit = Some(fee_setting.base_fee * 20);
        }
    }

    pub fn check_sol_transaction_fee(
        &self,
        balance: U256,
        fee: u64,
    ) -> Result<(), crate::error::service::ServiceError> {
        let fee = U256::from(fee);
        if balance < fee {
            return Err(crate::error::business::BusinessError::Chain(
                crate::error::business::chain::ChainError::InsufficientFeeBalance,
            ))?;
        }
        Ok(())
    }

    fn native_transfer_rent_precheck(
        from: &str,
        to: &str,
        recipient_exists: bool,
        transfer_amount: U256,
        minimum_rent: U256,
    ) -> Result<(), crate::error::service::ServiceError> {
        if recipient_exists || transfer_amount >= minimum_rent {
            return Ok(());
        }

        Err(crate::error::business::BusinessError::Chain(
            crate::error::business::chain::ChainError::insufficient_balance_with_detail(
                crate::error::business::chain::InsufficientBalanceDetail::new()
                    .from_addr(from.to_string())
                    .to_addr(to.to_string())
                    .chain_code("sol")
                    .value(transfer_amount.to_string())
                    .balance(transfer_amount.to_string())
                    .need(minimum_rent.to_string())
                    .reason(
                        "recipient account is not initialized and transfer amount is below rent-exempt minimum",
                    ),
            ),
        ))?
    }

    async fn check_native_transfer_rent(
        &self,
        from: &str,
        to: &str,
        transfer_amount: U256,
    ) -> Result<(), crate::error::service::ServiceError> {
        let to = wallet_utils::address::parse_sol_address(to)?;
        let account = self.chain.get_provider().account_info(to).await?;
        let recipient_exists = account.value.is_some();
        let minimum_rent =
            wallet_utils::unit::convert_to_u256(&SYSTEM_ACCOUNT_RENT.to_string(), SOL_DECIMAL)?;

        Self::native_transfer_rent_precheck(
            from,
            &to.to_string(),
            recipient_exists,
            transfer_amount,
            minimum_rent,
        )
    }
}

#[async_trait::async_trait]
impl Tx for SolTx {
    async fn account_resource(
        &self,
        _owner_address: &str,
    ) -> Result<AccountResourceDetail, ServiceError> {
        todo!()
    }

    async fn balance_token_key(&self, addr: &str, token: AssetTokenKey) -> Result<U256, Error> {
        self.chain.balance(addr, token.to_chain_token_option()).await
    }

    async fn nonce(&self, addr: &str) -> Result<u64, ServiceError> {
        Ok(0)
    }

    async fn block_num(&self) -> Result<u64, Error> {
        self.chain.block_num().await
    }

    async fn query_tx_res(&self, hash: &str) -> Result<Option<QueryTransactionResult>, Error> {
        self.chain.query_tx_res(hash).await
    }

    async fn query_tx_seen_on_node(&self, hash: &str) -> Result<bool, ServiceError> {
        Ok(self.chain.query_tx_res(hash).await?.is_some())
    }

    async fn token_symbol(&self, token: &str) -> Result<String, Error> {
        self.chain.token_symbol(token).await
    }

    async fn token_name(&self, token: &str) -> Result<String, Error> {
        self.chain.token_name(token).await
    }

    async fn decimals(&self, token: &str) -> Result<u8, Error> {
        self.chain.decimals(token).await
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
        // check balance
        let token_key = params.base.token_address.clone();
        let token = token_key.to_chain_token_option();
        let balance = self.chain.balance(&params.base.from, None).await?;
        let remain_balance = self
            .check_sol_balance(&params.base.from, balance, token.as_deref(), transfer_amount)
            .await?;
        if token.is_none() {
            self.check_native_transfer_rent(&params.base.from, &params.base.to, transfer_amount).await?;
        }

        let params = TransferOpt::new(
            &params.base.from,
            &params.base.to,
            &params.base.value,
            token.clone(),
            params.base.decimals,
            self.chain.get_provider(),
        )?;

        let instructions = params.instructions().await?;
        let mut fee_setting = self.chain.estimate_fee_v1(&instructions, &params).await?;
        self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

        self.check_sol_transaction_fee(remain_balance, fee_setting.original_fee())?;
        let fee = fee_setting.transaction_fee().to_string();

        let tx_hash = self
            .chain
            .exec_transaction(params, private_key, Some(fee_setting), instructions, 0)
            .await?;

        Ok(TransferResp::new(tx_hash, fee))
    }

    async fn build_transfer_raw(
        &self,
        params: &ApiTransferReq,
        private_key: ChainPrivateKey,
    ) -> Result<(String, RawTx, String), crate::error::service::ServiceError> {
        let transfer_amount = self.check_min_transfer(&params.base.value, params.base.decimals)?;
        // check balance
        let token_key = params.base.token_address.clone();
        let token = token_key.to_chain_token_option();
        let balance = self.chain.balance(&params.base.from, None).await?;
        let remain_balance = self
            .check_sol_balance(&params.base.from, balance, token.as_deref(), transfer_amount)
            .await?;
        if token.is_none() {
            self.check_native_transfer_rent(&params.base.from, &params.base.to, transfer_amount).await?;
        }

        let params = TransferOpt::new(
            &params.base.from,
            &params.base.to,
            &params.base.value,
            token.clone(),
            params.base.decimals,
            self.chain.get_provider(),
        )?;

        let instructions = params.instructions().await?;
        let mut fee_setting = self.chain.estimate_fee_v1(&instructions, &params).await?;
        self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

        self.check_sol_transaction_fee(remain_balance, fee_setting.original_fee())?;
        let fee = fee_setting.transaction_fee().to_string();

        let (tx_hash, raw_tx) = self
            .chain
            .build_legacy_signed_tx(params, private_key, Some(fee_setting), instructions)
            .await?;

        Ok((tx_hash, RawTx::Sol(raw_tx, fee.clone()), fee))
    }

    async fn broadcast_transfer(
        &self,
        raw: RawTx,
    ) -> Result<TransferResp, crate::error::service::ServiceError> {
        if let RawTx::Sol(raw, fee) = raw {
            let tx_hash = self.chain.get_provider().broadcast_legacy(&raw).await?;
            Ok(TransferResp::new(tx_hash, fee))
        } else {
            Err(ServiceError::Business(BusinessError::Chain(ChainError::InvalidRawTx)))
        }
    }

    async fn estimate_fee(
        &self,
        req: ApiBaseTransferReq,
        main_symbol: &str,
    ) -> Result<String, ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();
        let token_currency = TokenCurrencyGetter::get_currency_by_token_key(
            currency,
            &req.chain_code,
            main_symbol,
            wallet_database::entities::asset_token_key::AssetTokenKey::Native,
        )
        .await?;

        let token_key = req.token_address.clone();
        let token = token_key.to_chain_token_option();
        let params = TransferOpt::new(
            &req.from,
            &req.to,
            &req.value,
            token.clone(),
            req.decimals,
            self.chain.get_provider(),
        )?;

        let instructions = params.instructions().await?;
        let mut fee_setting = self.chain.estimate_fee_v1(&instructions, &params).await?;

        self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

        let res = CommonFeeDetails::new(fee_setting.transaction_fee(), token_currency, currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;
        Ok(fee)
    }

    // async fn approve(
    //     &self,
    //     _req: &ApproveReq,
    //     _key: ChainPrivateKey,
    //     _value: U256,
    // ) -> Result<TransferResp, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn approve_fee(
    //     &self,
    //     _req: &ApproveReq,
    //     _value: U256,
    //     _main_symbol: &str,
    // ) -> Result<String, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn allowance(
    //     &self,
    //     _from: &str,
    //     _token: &str,
    //     _spender: &str,
    // ) -> Result<U256, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn swap_quote(
    //     &self,
    //     _req: &QuoteReq,
    //     _quote_resp: &AggQuoteResp,
    //     _symbol: &str,
    // ) -> Result<(U256, String, String), ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn swap(
    //     &self,
    //     _req: &SwapReq,
    //     _fee: String,
    //     _key: ChainPrivateKey,
    // ) -> Result<TransferResp, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn deposit_fee(
    //     &self,
    //     _req: DepositReq,
    //     _main_coin: &CoinEntity,
    // ) -> Result<(String, String), ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn deposit(
    //     &self,
    //     _req: &DepositReq,
    //     _fee: String,
    //     _key: ChainPrivateKey,
    //     value: U256,
    // ) -> Result<TransferResp, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn withdraw_fee(
    //     &self,
    //     _req: WithdrawReq,
    //     _main_coin: &CoinEntity,
    // ) -> Result<(String, String), ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }

    // async fn withdraw(
    //     &self,
    //     _req: &WithdrawReq,
    //     _fee: String,
    //     _key: ChainPrivateKey,
    //     _value: U256,
    // ) -> Result<TransferResp, ServiceError> {
    //     Err(crate::error::business::BusinessError::Chain(
    //         crate::error::business::chain::ChainError::NotSupportChain,
    //     )
    //     .into())
    // }
}

#[cfg(test)]
mod tests {
    use super::{SYSTEM_ACCOUNT_RENT, SolTx};
    use alloy::primitives::U256;
    use wallet_chain_interact::sol::consts::SOL_DECIMAL;

    fn minimum_rent() -> U256 {
        wallet_utils::unit::convert_to_u256(&SYSTEM_ACCOUNT_RENT.to_string(), SOL_DECIMAL)
            .expect("system rent should convert")
    }

    #[test]
    fn sol_native_recipient_rent_precheck_passes_for_existing_account() {
        let res = SolTx::native_transfer_rent_precheck(
            "from",
            "to",
            true,
            U256::from(1_u64),
            minimum_rent(),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn sol_native_recipient_rent_precheck_fails_for_missing_account_and_small_amount() {
        let res = SolTx::native_transfer_rent_precheck(
            "from",
            "to",
            false,
            U256::from(15_000_u64),
            minimum_rent(),
        );
        let err = res.expect_err("expected rent precheck to fail");
        let msg = err.to_string();
        assert!(msg.contains("recipient account is not initialized"));
        assert!(msg.contains("rent-exempt minimum"));
    }

    #[test]
    fn sol_native_recipient_rent_precheck_allows_missing_account_when_amount_is_large_enough() {
        let res =
            SolTx::native_transfer_rent_precheck("from", "to", false, minimum_rent(), minimum_rent());
        assert!(res.is_ok());
    }
}

// #[async_trait::async_trait]
// impl Multisig for SolTx {
//     async fn multisig_address(
//         &self,
//         _account: &MultisigAccountEntity,
//         _member: &MultisigMemberEntities,
//     ) -> Result<FetchMultisigAddressResp, ServiceError> {
//         Ok(MultisigAccountOpt::multisig_address()?)
//     }
//
//     async fn deploy_multisig_account(
//         &self,
//         account: &MultisigAccountEntity,
//         member: &MultisigMemberEntities,
//         _fee_setting: Option<String>,
//         key: ChainPrivateKey,
//     ) -> Result<(String, String), ServiceError> {
//         let params = MultisigAccountOpt::new(
//             &account.initiator_addr,
//             account.threshold as u8,
//             member.get_owner_str_vec(),
//             account.salt.clone(),
//             self.chain.get_provider(),
//         )?;
//
//         let instructions = params.instructions().await?;
//
//         // check transaction_fee
//         let fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//         let balance = self.chain.balance(&account.initiator_addr, None).await?;
//         self.check_sol_transaction_fee(balance, fee.original_fee())?;
//
//         let tx_hash = self.chain.exec_transaction(params, key, None, instructions, 0).await?;
//
//         Ok((tx_hash, "".to_string()))
//     }
//
//     async fn deploy_multisig_fee(
//         &self,
//         account: &MultisigAccountEntity,
//         member: MultisigMemberEntities,
//         main_symbol: &str,
//     ) -> Result<String, ServiceError> {
//         let currency_lock = crate::app_state::APP_STATE.read().await;
//         let currency = currency_lock.currency();
//         let token_currency =
//             TokenCurrencyGetter::get_currency(currency, &account.chain_code, main_symbol, None)
//                 .await?;
//
//         let owners = member.get_owner_str_vec();
//
//         let salt = TEMP_SOL_KEYPAIR;
//         let params = MultisigAccountOpt::new(
//             &account.initiator_addr,
//             account.threshold as u8,
//             owners,
//             salt.to_string(),
//             self.chain.get_provider(),
//         )?;
//
//         let instructions = params.instructions().await?;
//         // check transaction_fee
//         let fee = self.chain.estimate_fee_v1(&instructions, &params).await?.transaction_fee();
//
//         CommonFeeDetails::new(fee, token_currency, currency)?.to_json_str()
//     }
//
//     async fn build_multisig_fee(
//         &self,
//         req: &MultisigQueueFeeParams,
//         account: &MultisigAccountEntity,
//         decimal: u8,
//         token: Option<String>,
//         main_symbol: &str,
//     ) -> Result<String, ServiceError> {
//         let currency = crate::app_state::APP_STATE.read().await;
//         let currency = currency.currency();
//
//         let token_currency =
//             TokenCurrencyGetter::get_currency(currency, &req.chain_code, main_symbol, None).await?;
//
//         let base = TransferOpt::new(
//             &req.from,
//             &req.to,
//             &req.value,
//             token.clone(),
//             decimal,
//             self.chain.get_provider(),
//         )?;
//
//         let params = BuildTransactionOpt::new(
//             &account.authority_addr,
//             account.member_num as usize,
//             &account.initiator_addr,
//             base,
//         )?;
//
//         // transaction params
//         let args = params.build_transaction_arg().await?;
//         let instructions = params.instructions(&args).await?;
//
//         // create transaction fee
//         let base_fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//         let mut fee_setting =
//             params.create_transaction_fee(&args.transaction_message, base_fee).await?;
//
//         self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);
//
//         let fee = CommonFeeDetails::new(fee_setting.transaction_fee(), token_currency, currency)?;
//         Ok(serde_func::serde_to_string(&fee)?)
//     }
//
//     async fn build_multisig_with_account(
//         &self,
//         req: &TransferParams,
//         account: &MultisigAccountEntity,
//         assets: &ApiAssetsEntity,
//         key: ChainPrivateKey,
//     ) -> Result<MultisigTxResp, ServiceError> {
//         let decimal = assets.decimals;
//         let token = assets.token_address();
//         let value = self.check_min_transfer(&req.value, decimal)?;
//
//         // check multisig account balance
//         let multisig_balance = self.chain.balance(&req.from, token.clone()).await?;
//         if multisig_balance < value {
//             return Err(crate::error::business::BusinessError::Chain(
//                 crate::error::business::chain::ChainError::insufficient_balance(),
//             ))?;
//         }
//         let base = TransferOpt::new(
//             &req.from,
//             &req.to,
//             &req.value,
//             token,
//             decimal,
//             self.chain.get_provider(),
//         )?;
//
//         let params = BuildTransactionOpt::new(
//             &account.authority_addr,
//             account.member_num as usize,
//             &account.initiator_addr,
//             base,
//         )?;
//
//         // transaction params
//         let args = params.build_transaction_arg().await?;
//         let instructions = params.instructions(&args).await?;
//
//         // create transaction fee
//         let base_fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//         let fee = params.create_transaction_fee(&args.transaction_message, base_fee).await?;
//         // check balance
//         let balance = self.chain.balance(&account.initiator_addr, None).await?;
//         self.check_sol_transaction_fee(balance, fee.original_fee())?;
//
//         // execute build transfer transaction
//         let pda = params.multisig_pda;
//         let tx_hash = self.chain.exec_transaction(params, key, None, instructions, 0).await?;
//
//         Ok(args.get_raw_data(pda, tx_hash)?)
//     }
//
//     async fn build_multisig_with_permission(
//         &self,
//         _req: &TransferParams,
//         _p: &PermissionEntity,
//         _coin: &CoinEntity,
//     ) -> Result<MultisigTxResp, ServiceError> {
//         Err(crate::error::business::BusinessError::Permission(
//             crate::error::business::permission::PermissionError::UnSupportPermissionChain,
//         )
//         .into())
//     }
//
//     async fn sign_fee(
//         &self,
//         account: &MultisigAccountEntity,
//         address: &str,
//         raw_data: &str,
//         main_symbol: &str,
//     ) -> Result<String, ServiceError> {
//         let currency = crate::app_state::APP_STATE.read().await;
//         let currency = currency.currency();
//
//         let params = SignTransactionOpt::new(address, raw_data.to_string())?;
//
//         let instructions = params.instructions().await?;
//         let fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//
//         let token_currency =
//             TokenCurrencyGetter::get_currency(currency, &account.chain_code, main_symbol, None)
//                 .await?;
//
//         let fee = CommonFeeDetails::new(fee.transaction_fee(), token_currency, currency)?;
//         Ok(serde_func::serde_to_string(&fee)?)
//     }
//
//     async fn sign_multisig_tx(
//         &self,
//         _account: &MultisigAccountEntity,
//         address: &str,
//         key: ChainPrivateKey,
//         raw_data: &str,
//     ) -> Result<MultisigSignResp, ServiceError> {
//         let balance = self.chain.balance(address, None).await?;
//         let params = SignTransactionOpt::new(address, raw_data.to_string())?;
//
//         let instructions = params.instructions().await?;
//         let fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//         self.check_sol_transaction_fee(balance, fee.original_fee())?;
//
//         Ok(self.chain.sign_with_res(instructions, params, key).await?)
//     }
//
//     async fn estimate_multisig_fee(
//         &self,
//         queue: &MultisigQueueEntity,
//         coin: &CoinEntity,
//         backend: &BackendApi,
//         sign_list: Vec<String>,
//         main_symbol: &str,
//     ) -> Result<String, ServiceError> {
//         let currency = crate::app_state::APP_STATE.read().await;
//         let currency = currency.currency();
//
//         let token_currency =
//             TokenCurrencyGetter::get_currency(currency, &queue.chain_code, main_symbol, None)
//                 .await?;
//
//         let params = ExecMultisigOpt::new(&queue.from_addr, queue.raw_data.to_string())?;
//
//         let instructions = params.instructions().await?;
//         let mut fee = self.chain.estimate_fee_v1(&instructions, &params).await?;
//         ChainTransDomain::sol_priority_fee(&mut fee, queue.token_addr.as_ref(), 200_000);
//
//         CommonFeeDetails::new(fee.transaction_fee(), token_currency, currency)?.to_json_str()
//     }
// }
