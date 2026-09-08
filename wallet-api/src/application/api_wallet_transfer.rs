use crate::{
    context::Context,
    domain::{
        api_wallet::{coin::ApiCoinDomain, trans::ApiTransDomain},
        chain::TransferResp,
        wallet::WalletDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::nonce::nonce_engine::get_nonce_engine_with_ctx,
    request::api_wallet::{
        trans::{ApiBaseTransferReq, ApiTransferReq},
        transfer::ApiTransferExReq,
    },
    response_vo::standard_wallet::transaction::TransactionResult,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::{
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawStatus, SignedSelfTransfer},
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, nonce::ApiNonceRepo, wallet::ApiWalletRepo,
        withdraw::ApiWithdrawRepo,
    },
};
use wallet_types::chain::chain::ChainCode;

pub(crate) struct ApiTransferApplication {
    ctx: &'static Context,
}

impl ApiTransferApplication {
    pub(crate) fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub(crate) async fn transfer(
        &self,
        params: ApiTransferExReq,
    ) -> Result<TransactionResult, ServiceError> {
        WalletDomain::validate_password_with_context(self.ctx, &params.password).await?;

        let private_key =
            crate::domain::api_wallet::account::ApiAccountDomain::get_private_key_with_ctx(
                self.ctx,
                &params.base.from,
                &params.base.chain_code,
            )
            .await?;
        self.transfer_with_private_key(params, private_key).await
    }

    pub(crate) async fn transfer_with_private_key(
        &self,
        params: ApiTransferExReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransactionResult, ServiceError> {
        let from_addr = params.base.from.clone();
        let _gate = get_nonce_engine_with_ctx(self.ctx)?
            .acquire_transfer_gate(&from_addr, &params.base.chain_code)
            .await;

        let pool = self.ctx.api_wallet_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params.base.from,
            &params.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(ServiceError::Business(ApiWalletError::NotFoundAccount.into()))?;
        let wallet = ApiWalletRepo::find_by_address(&pool, &account.wallet_address).await?.ok_or(
            ServiceError::Business(
                ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )
                .into(),
            ),
        )?;

        let token_key = params.base.token_address.clone();
        let coin = ApiCoinDomain::get_coin_by_token_key_exact_with_ctx(
            self.ctx,
            &params.base.chain_code,
            token_key.clone(),
        )
        .await?;

        let chain_code: ChainCode = params.base.chain_code.as_str().try_into()?;
        let nonce =
            self.get_transfer_nonce(&params.base.from, &params.base.chain_code, chain_code).await?;

        let req = ApiTransferReq {
            base: Self::build_api_transfer_base(&params, &token_key, coin.decimals),
            password: params.password.to_string(),
            nonce: nonce as u64,
        };
        if matches!(chain_code, ChainCode::Ethereum | ChainCode::BnbSmartChain) {
            let (tx_hash, raw, fee) =
                ApiTransDomain::build_transfer_raw(self.ctx, req, Some(private_key)).await?;
            let raw_json = wallet_utils::serde_func::serde_to_string(&raw)?;
            let trade_no = uuid::Uuid::new_v4().to_string();
            let pool = self.ctx.api_transaction_pool()?;
            // Persist all signed facts before the first send; a timeout or crash
            // must leave the same bytes available for later reconciliation.
            ApiWithdrawRepo::persist_signed_self_transfer(
                &pool,
                &SignedSelfTransfer {
                    trade_no: &trade_no,
                    uid: &wallet.uid,
                    name: &wallet.name,
                    from_addr: &params.base.from,
                    to_addr: &params.base.to,
                    chain_code: &params.base.chain_code,
                    token_addr: token_key,
                    symbol: &params.base.symbol,
                    value: &params.base.value,
                    nonce,
                    tx_hash: &tx_hash,
                    raw_tx: &raw_json,
                    transaction_fee: &fee,
                    notes: params.base.notes.as_deref(),
                },
            )
            .await?;
            let response = ApiTransDomain::broadcast_transfer(
                self.ctx,
                &params.base.chain_code,
                raw,
                Some(&tx_hash),
            )
            .await?;
            let Some(response) = response else {
                return Err(ServiceError::Parameter(format!(
                    "transfer_broadcast_uncertain: tx_hash={tx_hash}"
                )));
            };
            if !response.tx_hash.eq_ignore_ascii_case(&tx_hash) {
                return Err(ServiceError::Parameter(format!(
                    "transfer_broadcast_hash_mismatch: expected={tx_hash}"
                )));
            }
            if !ApiWithdrawRepo::mark_self_transfer_accepted(&pool, &trade_no, &tx_hash).await? {
                return Err(ServiceError::Parameter(format!(
                    "transfer_acceptance_persistence_conflict: tx_hash={tx_hash}"
                )));
            }
            if let Some(request_id) = &params.base.request_resource_id {
                let _ = self.ctx.get_global_backend_api().delegate_complete(request_id).await;
            }
            return Ok(TransactionResult { tx_hash });
        }

        let res = ApiTransDomain::transfer_with_ctx(self.ctx, req, Some(private_key)).await?;
        self.record_transfer(params, token_key, nonce, &wallet.uid, &wallet.name, &res).await?;
        Ok(TransactionResult { tx_hash: res.tx_hash })
    }

    pub(crate) fn build_api_transfer_base(
        params: &ApiTransferExReq,
        token_key: &AssetTokenKey,
        decimals: u8,
    ) -> ApiBaseTransferReq {
        ApiBaseTransferReq {
            from: params.base.from.clone(),
            to: params.base.to.clone(),
            value: params.base.value.clone(),
            chain_code: params.base.chain_code.clone(),
            token_address: token_key.clone(),
            decimals,
            symbol: params.base.symbol.clone(),
            request_resource_id: params.base.request_resource_id.clone(),
            spend_all: params.base.spend_all,
            notes: params.base.notes.clone(),
            metadata: Some(params.fee_setting.clone()),
        }
    }

    async fn get_eth_nonce(&self, from_addr: &str, chain_code: &str) -> Result<i64, ServiceError> {
        let nonce_engine = get_nonce_engine_with_ctx(self.ctx)?;
        let nonce = nonce_engine.allocate_ordinary_transfer_nonce(from_addr, chain_code).await?;
        Ok(nonce as i64)
    }

    async fn get_transfer_nonce(
        &self,
        from_addr: &str,
        chain_code: &str,
        chain_code_enum: ChainCode,
    ) -> Result<i64, ServiceError> {
        match chain_code_enum {
            ChainCode::Ethereum | ChainCode::BnbSmartChain => {
                self.get_eth_nonce(from_addr, chain_code).await
            }
            _ => Ok(0),
        }
    }

    async fn record_transfer(
        &self,
        params: ApiTransferExReq,
        token_key: AssetTokenKey,
        nonce: i64,
        wallet_uid: &str,
        wallet_name: &str,
        res: &TransferResp,
    ) -> Result<(), ServiceError> {
        let api_transaction_pool = self.ctx.api_transaction_pool()?;
        let resource_consume = res.resource_consume().unwrap_or_else(|_| "".to_string());
        let trade_no = uuid::Uuid::new_v4().to_string();
        ApiWithdrawRepo::upsert_api_withdraw(
            &api_transaction_pool,
            wallet_uid,
            wallet_name,
            &params.base.from,
            &params.base.to,
            &params.base.value,
            "",
            &params.base.chain_code,
            token_key,
            &params.base.symbol,
            &trade_no,
            None,
            None,
            None,
            ApiTradeType::SelfWithdraw,
            nonce,
            Some(res.tx_hash.clone()),
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            resource_consume.as_str(),
            res.fee.as_str(),
            None,
            None,
        )
        .await?;
        ApiNonceRepo::set_nonce_floor(
            &api_transaction_pool,
            &params.base.from,
            &params.base.chain_code,
            nonce,
        )
        .await?;
        Ok(())
    }
}
