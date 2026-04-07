use crate::{
    domain::{
        api_wallet::adapter::{
            TIME_OUT,
            tx::{RawTx, Tx},
        },
        chain::{
            TransferResp,
            adapter::sol_tx::{SYSTEM_ACCOUNT_RENT, TOKEN_ACCOUNT_RENT},
            transaction::DEFAULT_UNITS,
        },
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

    pub(crate) fn native_transfer_rent_precheck(
        from: &str,
        to: &str,
        recipient_exists: bool,
        payer_balance: U256,
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
                    .balance(payer_balance.to_string())
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
        payer_balance: U256,
        transfer_amount: U256,
    ) -> Result<(), crate::error::service::ServiceError> {
        let to_addr = wallet_utils::address::parse_sol_address(to)?;
        let account = self.chain.get_provider().account_info(to_addr).await?;
        let recipient_exists = account.value.is_some();
        let minimum_rent =
            wallet_utils::unit::convert_to_u256(&SYSTEM_ACCOUNT_RENT.to_string(), SOL_DECIMAL)?;

        Self::native_transfer_rent_precheck(
            from,
            to,
            recipient_exists,
            payer_balance,
            transfer_amount,
            minimum_rent,
        )
    }

    fn apply_token_recipient_ata_rent(fee_setting: &mut SolFeeSetting, recipient_exists: bool) {
        if recipient_exists {
            return;
        }

        let extra_fee = fee_setting.extra_fee.unwrap_or_default();
        fee_setting.extra_fee = Some(extra_fee.saturating_add(TOKEN_ACCOUNT_RENT));
    }

    fn sol_fee_balance_reserve(fee_setting: &SolFeeSetting) -> u64 {
        fee_setting.original_fee().saturating_add(fee_setting.extra_fee.unwrap_or_default())
    }

    async fn reserve_token_recipient_ata_rent(
        &self,
        to: &str,
        token: &str,
        fee_setting: &mut SolFeeSetting,
    ) -> Result<bool, crate::error::service::ServiceError> {
        let account = self.chain.get_provider().token_balance(token, to).await?;
        let recipient_exists = !account.value.is_empty();
        if recipient_exists {
            tracing::info!(
                to = %to,
                token = %token,
                source = "sol_tx",
                "token recipient ATA already exists"
            );
        } else {
            tracing::info!(
                to = %to,
                token = %token,
                ata_rent = TOKEN_ACCOUNT_RENT,
                source = "sol_tx",
                "token recipient ATA missing; reserving ATA rent"
            );
            Self::apply_token_recipient_ata_rent(fee_setting, recipient_exists);
        }

        Ok(recipient_exists)
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
        let to = params.base.to.clone();
        let balance = self.chain.balance(&params.base.from, None).await?;
        let remain_balance = self
            .check_sol_balance(&params.base.from, balance, token.as_deref(), transfer_amount)
            .await?;
        if token.is_none() {
            self.check_native_transfer_rent(&params.base.from, &to, balance, transfer_amount)
                .await?;
        }

        let params = TransferOpt::new(
            &params.base.from,
            &to,
            &params.base.value,
            token.clone(),
            params.base.decimals,
            self.chain.get_provider(),
        )?;

        let instructions = params.instructions().await?;
        let mut fee_setting = self.chain.estimate_fee_v1(&instructions, &params).await?;
        self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

        if let Some(token) = token.as_ref() {
            self.reserve_token_recipient_ata_rent(&to, token, &mut fee_setting).await?;
        }

        self.check_sol_transaction_fee(
            remain_balance,
            Self::sol_fee_balance_reserve(&fee_setting),
        )?;
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
        let to = params.base.to.clone();
        let balance = self.chain.balance(&params.base.from, None).await?;
        let remain_balance = self
            .check_sol_balance(&params.base.from, balance, token.as_deref(), transfer_amount)
            .await?;
        if token.is_none() {
            self.check_native_transfer_rent(&params.base.from, &to, balance, transfer_amount)
                .await?;
        }

        let params = TransferOpt::new(
            &params.base.from,
            &to,
            &params.base.value,
            token.clone(),
            params.base.decimals,
            self.chain.get_provider(),
        )?;

        let instructions = params.instructions().await?;
        let mut fee_setting = self.chain.estimate_fee_v1(&instructions, &params).await?;
        self.sol_priority_fee(&mut fee_setting, token.as_ref(), DEFAULT_UNITS);

        if let Some(token) = token.as_ref() {
            self.reserve_token_recipient_ata_rent(&to, token, &mut fee_setting).await?;
        }

        self.check_sol_transaction_fee(
            remain_balance,
            Self::sol_fee_balance_reserve(&fee_setting),
        )?;
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

        let transfer_amount = self.check_min_transfer(&req.value, req.decimals)?;
        let token_key = req.token_address.clone();
        let token = token_key.to_chain_token_option();
        let balance = self.chain.balance(&req.from, None).await?;
        let remain_balance =
            self.check_sol_balance(&req.from, balance, token.as_deref(), transfer_amount).await?;
        if token.is_none() {
            self.check_native_transfer_rent(&req.from, &req.to, balance, transfer_amount).await?;
        }

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
        if let Some(token) = token.as_ref() {
            self.reserve_token_recipient_ata_rent(&req.to, token, &mut fee_setting).await?;
        }

        self.check_sol_transaction_fee(
            remain_balance,
            Self::sol_fee_balance_reserve(&fee_setting),
        )?;
        let fee = fee_setting.transaction_fee();
        let res = CommonFeeDetails::new(fee, token_currency, currency)?;
        let fee = wallet_utils::serde_func::serde_to_string(&res)?;
        Ok(fee)
    }

    async fn estimate_fee_without_balance_check(
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

        let _transfer_amount = self.check_min_transfer(&req.value, req.decimals)?;
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
        if let Some(token) = token.as_ref() {
            self.reserve_token_recipient_ata_rent(&req.to, token, &mut fee_setting).await?;
        }

        let fee = fee_setting.transaction_fee();
        let res = CommonFeeDetails::new(fee, token_currency, currency)?;
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
    use super::SolTx;
    use crate::{
        domain::{api_wallet::adapter::tx::Tx, chain::adapter::sol_tx::TOKEN_ACCOUNT_RENT},
        request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq},
        test::env::get_manager,
    };
    use alloy::primitives::U256;
    use serde::Deserialize;
    use std::path::Path;
    use wallet_chain_interact::types::ChainPrivateKey;
    use wallet_database::entities::asset_token_key::AssetTokenKey;

    const SOL_SMOKE_CONFIG_PATH: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/transactions/sol_smoke.local.toml");

    #[derive(Debug, Deserialize)]
    struct SolSmokeConfig {
        rpc_url: String,
        from: String,
        to: String,
        amount: String,
        private_key: String,
        token_mint: Option<String>,
        token_decimals: Option<u8>,
        symbol: Option<String>,
    }

    fn build_transfer_req(
        from: &str,
        to: &str,
        value: &str,
        token_address: AssetTokenKey,
        decimals: u8,
        symbol: &str,
    ) -> ApiTransferReq {
        ApiTransferReq {
            base: ApiBaseTransferReq {
                from: from.to_string(),
                to: to.to_string(),
                value: value.to_string(),
                chain_code: "sol".to_string(),
                token_address,
                decimals,
                symbol: symbol.to_string(),
                request_resource_id: None,
                spend_all: false,
                notes: None,
                metadata: None,
            },
            password: String::new(),
            nonce: 0,
        }
    }

    fn load_sol_smoke_config() -> Option<SolSmokeConfig> {
        let path = Path::new(SOL_SMOKE_CONFIG_PATH);
        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;
        wallet_utils::serde_func::toml_from_str(&content).ok()
    }

    #[test]
    fn sol_native_recipient_rent_precheck_passes_for_existing_account() {
        let res = SolTx::native_transfer_rent_precheck(
            "from",
            "to",
            true,
            U256::from(1_u64),
            U256::from(1_u64),
            U256::from(1_u64),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn sol_native_recipient_rent_precheck_fails_for_missing_account_and_small_amount() {
        let res = SolTx::native_transfer_rent_precheck(
            "from",
            "to",
            false,
            U256::from(7_309_206_u64),
            U256::from(15_000_u64),
            U256::from(990_880_u64),
        );
        let err = res.expect_err("expected rent precheck to fail");
        let msg = err.to_string();
        assert!(msg.contains("recipient account is not initialized"));
        assert!(msg.contains("rent-exempt minimum"));
    }

    #[test]
    fn sol_native_recipient_rent_precheck_allows_missing_account_when_amount_is_large_enough() {
        let res = SolTx::native_transfer_rent_precheck(
            "from",
            "to",
            false,
            U256::from(990_880_u64),
            U256::from(990_880_u64),
            U256::from(990_880_u64),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn sol_token_recipient_ata_rent_is_reserved_when_account_is_missing() {
        let mut fee_setting = wallet_chain_interact::sol::SolFeeSetting {
            base_fee: 100,
            priority_fee_per_compute_unit: None,
            compute_units_consumed: 0,
            extra_fee: None,
        };

        SolTx::apply_token_recipient_ata_rent(&mut fee_setting, false);

        assert_eq!(fee_setting.extra_fee, Some(TOKEN_ACCOUNT_RENT));
    }

    #[test]
    fn sol_token_recipient_ata_rent_is_not_reserved_when_account_exists() {
        let mut fee_setting = wallet_chain_interact::sol::SolFeeSetting {
            base_fee: 100,
            priority_fee_per_compute_unit: None,
            compute_units_consumed: 0,
            extra_fee: None,
        };

        SolTx::apply_token_recipient_ata_rent(&mut fee_setting, true);

        assert_eq!(fee_setting.extra_fee, None);
    }

    #[tokio::test]
    #[ignore = "requires live Solana RPC plus funded sender private key"]
    async fn sol_transfer_onchain_smoke() {
        let config = load_sol_smoke_config().unwrap_or_else(|| {
            panic!(
                "create wallet-api/tests/transactions/sol_smoke.local.toml with rpc_url, from, to, amount, private_key, and optional token_mint/token_decimals/symbol"
            )
        });

        let (wallet_manager, _params) = get_manager().await.expect("create test wallet manager");
        wallet_manager.init_api_swap().await.expect("init api swap should succeed");

        let header_opt = Some(
            crate::context::CONTEXT
                .get()
                .expect("context should be initialized")
                .get_rpc_header()
                .await
                .expect("rpc header should be available"),
        );

        let (token_address, decimals, symbol) = match config.token_mint.as_deref() {
            Some(mint) if !mint.is_empty() => {
                let decimals = config.token_decimals.unwrap_or(6);
                let symbol = config.symbol.as_deref().unwrap_or("TOKEN");
                (AssetTokenKey::Contract(mint.to_string()), decimals, symbol)
            }
            None => (AssetTokenKey::Native, 9, "SOL"),
            Some(_) => (AssetTokenKey::Native, 9, "SOL"),
        };

        let sol_tx = SolTx::new(&config.rpc_url, header_opt)
            .expect("failed to create Solana client with rpc headers");
        let req = build_transfer_req(
            &config.from,
            &config.to,
            &config.amount,
            token_address,
            decimals,
            symbol,
        );
        let private_key: ChainPrivateKey = config.private_key.into();

        let resp = sol_tx.transfer(&req, private_key).await.expect("sol transfer should succeed");

        assert!(!resp.tx_hash.is_empty());
        assert!(!resp.fee.is_empty());
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
