use crate::{
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, adapter::tx::RawTx},
        chain::adapter::ChainAdapterFactory,
    },
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, account::AccountError, wallet::WalletError},
            chain::ChainError,
        },
        service::ServiceError,
    },
    messaging::notify::{
        FrontendNotifyEvent,
        event::NotifyEvent,
        other::{Process, TransactionProcessFrontend},
    },
    request::{
        stake::{FreezeBalanceReq, UnFreezeBalanceReq, VoteWitnessReq, WithdrawBalanceReq},
        transaction::Signer,
    },
    response_vo::standard_wallet::stake::{FreezeResp, ResourceResp},
};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::{
        self,
        operations::{
            TronTxOperation,
            stake::{
                FreezeBalanceArgs, ResourceType, UnFreezeBalanceArgs, VoteWitnessArgs,
                WithdrawBalanceArgs,
            },
        },
    },
};
use wallet_database::{
    entities::{api_account::ApiAccountEntity, api_wallet::ApiWalletType, bill::BillKind},
    repositories::api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo},
};

pub(crate) struct ApiResourceDomain;

pub(crate) struct ApiResourceBroadcastOutcome {
    pub(crate) uid: Option<String>,
    pub(crate) resource_type: Option<ResourceType>,
    pub(crate) amount: Option<String>,
    pub(crate) tx_hash: String,
    pub(crate) owner_address: String,
    pub(crate) raw_tx: String,
    pub(crate) transaction_fee: String,
    pub(crate) resp: Option<FreezeResp>,
}

pub(crate) struct ApiWithdrawWalletAccountContext {
    pub(crate) uid: String,
    pub(crate) owner_address: String,
}

impl ApiResourceDomain {
    pub(crate) async fn stake_withdraw_wallet_resource(
        ctx: &'static Context,
        req: &FreezeBalanceReq,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_ctx = Self::withdraw_wallet_account_context(ctx, &req.owner_address).await?;
        let frozen_balance_trx = Self::parse_amount_trx(req.frozen_balance)?;
        let resource_type = ResourceType::try_from(req.resource.as_str())?;
        let args = FreezeBalanceArgs::try_from(req)?;

        let bill_kind = match resource_type {
            ResourceType::BANDWIDTH => BillKind::FreezeBandwidth,
            ResourceType::ENERGY => BillKind::FreezeEnergy,
        };

        let mut outcome = Self::execute_tron_resource_operation(
            owner_ctx.owner_address,
            frozen_balance_trx,
            args,
            bill_kind,
            &req.signer,
            ctx,
        )
        .await?;
        outcome.uid = Some(owner_ctx.uid);
        outcome.resource_type = Some(resource_type);
        let resource_value =
            Self::resource_value(&outcome.owner_address, frozen_balance_trx, resource_type, ctx)
                .await?;
        let resource = ResourceResp::new(frozen_balance_trx, resource_type, resource_value);
        outcome.resp = Some(FreezeResp::new(
            outcome.owner_address.clone(),
            resource,
            outcome.tx_hash.clone(),
            bill_kind,
        ));
        Ok(outcome)
    }

    pub(crate) async fn unstake_withdraw_wallet_resource(
        ctx: &'static Context,
        req: &UnFreezeBalanceReq,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_ctx = Self::withdraw_wallet_account_context(ctx, &req.owner_address).await?;
        let unfreeze_balance_trx = Self::parse_amount_trx(req.unfreeze_balance)?;
        let resource_type = ResourceType::try_from(req.resource.as_str())?;
        let args = UnFreezeBalanceArgs::try_from(req)?;

        let bill_kind = match resource_type {
            ResourceType::BANDWIDTH => BillKind::UnFreezeBandwidth,
            ResourceType::ENERGY => BillKind::UnFreezeEnergy,
        };

        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(ctx).await?;
        let can_withdraw =
            chain.get_provider().can_withdraw_unfreeze_amount(&req.owner_address).await?;

        let mut outcome = Self::execute_tron_resource_operation(
            owner_ctx.owner_address,
            0,
            args,
            bill_kind,
            &req.signer,
            ctx,
        )
        .await?;
        outcome.uid = Some(owner_ctx.uid);
        outcome.resource_type = Some(resource_type);
        let resource_value =
            Self::resource_value(&outcome.owner_address, unfreeze_balance_trx, resource_type, ctx)
                .await?;
        let resource = ResourceResp::new(unfreeze_balance_trx, resource_type, resource_value);
        outcome.resp = Some(
            FreezeResp::new(
                outcome.owner_address.clone(),
                resource,
                outcome.tx_hash.clone(),
                bill_kind,
            )
            .expiration_at(wallet_utils::time::now_plus_days(14))
            .withdraw_amount(can_withdraw.to_sun()),
        );
        Ok(outcome)
    }

    pub(crate) async fn withdraw_wallet_votes(
        ctx: &'static Context,
        req: VoteWitnessReq,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_ctx = Self::withdraw_wallet_account_context(ctx, &req.owner_address).await?;
        let args = VoteWitnessArgs::try_from(&req)?;
        let mut outcome = Self::execute_tron_resource_operation(
            req.owner_address.clone(),
            0,
            args,
            BillKind::Vote,
            &req.signer,
            ctx,
        )
        .await?;
        outcome.uid = Some(owner_ctx.uid);
        outcome.resource_type = Some(ResourceType::BANDWIDTH);
        outcome.amount = Some(req.get_votes().to_string());
        Ok(outcome)
    }

    pub(crate) async fn withdraw_wallet_claim_votes_rewards(
        ctx: &'static Context,
        req: WithdrawBalanceReq,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_ctx = Self::withdraw_wallet_account_context(ctx, &req.owner_address).await?;
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(ctx).await?;
        let value = chain.get_provider().get_reward(&req.owner_address).await?.to_sun();
        if value < 0.0 {
            return Err(ServiceError::Business(BusinessError::Chain(ChainError::NoRewardClaim)));
        }
        let mut args = WithdrawBalanceArgs::try_from(&req)?;
        args.value = Some(value);
        let mut outcome = Self::execute_tron_resource_operation(
            req.owner_address.clone(),
            0,
            args,
            BillKind::WithdrawReward,
            &req.signer,
            ctx,
        )
        .await?;
        outcome.uid = Some(owner_ctx.uid);
        outcome.resource_type = Some(ResourceType::BANDWIDTH);
        outcome.amount = Some(value.to_string());
        Ok(outcome)
    }

    pub(crate) async fn withdraw_wallet_account_context(
        ctx: &'static Context,
        owner_address: &str,
    ) -> Result<ApiWithdrawWalletAccountContext, ServiceError> {
        let pool = ctx.api_wallet_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(owner_address, "tron", &pool)
            .await?
            .ok_or(ServiceError::Business(
                ApiWalletError::Account(AccountError::NotFound).into(),
            ))?;

        Self::account_context_from_entity(account)
    }

    pub(crate) async fn withdraw_wallet_address(
        ctx: &'static Context,
        withdraw_wallet_uid: &str,
    ) -> Result<String, ServiceError> {
        let pool = ctx.api_wallet_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, withdraw_wallet_uid)
            .await?
            .ok_or(ServiceError::Business(ApiWalletError::Wallet(WalletError::NotFound).into()))?;

        if wallet.api_wallet_type != ApiWalletType::Withdrawal {
            return Err(ServiceError::Business(
                ApiWalletError::Wallet(WalletError::WithdrawalWalletNotUsed).into(),
            ));
        }

        let tron_accounts =
            ApiAccountRepo::find_all_by_wallet_address_index(&pool, &wallet.address, "tron", 1)
                .await?;
        tron_accounts.into_iter().next().map(|account| account.address).ok_or_else(|| {
            ServiceError::Parameter(format!(
                "withdraw wallet tron account not found: uid={withdraw_wallet_uid}"
            ))
        })
    }

    fn account_context_from_entity(
        account: ApiAccountEntity,
    ) -> Result<ApiWithdrawWalletAccountContext, ServiceError> {
        if account.api_wallet_type != ApiWalletType::Withdrawal {
            return Err(ServiceError::Business(
                ApiWalletError::Wallet(WalletError::WithdrawalWalletNotUsed).into(),
            ));
        }

        Ok(ApiWithdrawWalletAccountContext { uid: account.uid, owner_address: account.address })
    }

    fn parse_amount_trx(amount: i64) -> Result<i64, ServiceError> {
        if amount <= 0 {
            return Err(ServiceError::Parameter(
                "api wallet resource amount must be positive".to_string(),
            ));
        }
        Ok(amount)
    }

    async fn execute_tron_resource_operation<T>(
        owner_address: String,
        stake_amount_trx: i64,
        args: impl TronTxOperation<T>,
        bill_kind: BillKind,
        signer: &Option<Signer>,
        ctx: &'static Context,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError>
    where
        T: Send + 'static,
    {
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(ctx).await?;
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send_with_ctx(ctx).await?;

        let raw_tx = args.build_raw_transaction(&chain.provider).await?;
        let balance = chain.balance(&owner_address, None).await?;
        let consumer = chain
            .get_provider()
            .transfer_fee(&owner_address, None, &raw_tx.raw_data_hex, 1)
            .await?;
        let need_sun = Self::required_balance_sun(consumer.transaction_fee_i64(), stake_amount_trx);
        if balance.to::<i64>() < need_sun {
            return Err(ServiceError::Business(BusinessError::Chain(
                ChainError::InsufficientBalance(Default::default()),
            )));
        }

        let signing_address =
            signer.as_ref().map(|signer| signer.address.as_str()).unwrap_or(&owner_address);
        let private_key =
            ApiAccountDomain::get_private_key_with_ctx(ctx, signing_address, "tron").await?;
        let resource_consume =
            BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0).to_json_str()?;
        let sign = wallet_utils::sign::sign_tron(&raw_tx.tx_id, &private_key, None)?;
        let mut raw_tx = raw_tx;
        raw_tx.signature.push(sign);
        let tx_hash = raw_tx.tx_id.clone();
        let transaction_fee = consumer.transaction_fee();
        let raw_tx = RawTx::Tron(
            raw_tx,
            BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0),
            transaction_fee.clone(),
        );
        let raw_tx_str = wallet_utils::serde_func::serde_to_string(&raw_tx)?;
        let RawTx::Tron(raw_tx, ..) = raw_tx else {
            unreachable!("resource operation builds tron tx")
        };
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send_with_ctx(ctx).await?;

        let broadcast_hash = chain.get_provider().exec_raw_transaction(raw_tx).await?.tx_id;
        if broadcast_hash != tx_hash {
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "api wallet resource tx_hash mismatch after broadcast".to_string(),
            )));
        }
        tracing::info!(
            owner_address = %owner_address,
            tx_hash = %tx_hash,
            transaction_fee = %transaction_fee,
            resource_consume = %resource_consume,
            "API wallet foreground resource operation broadcasted"
        );

        Ok(ApiResourceBroadcastOutcome {
            uid: None,
            resource_type: None,
            amount: None,
            tx_hash,
            owner_address,
            raw_tx: raw_tx_str,
            transaction_fee,
            resp: None,
        })
    }

    async fn resource_value(
        owner_address: &str,
        amount: i64,
        resource_type: ResourceType,
        ctx: &'static Context,
    ) -> Result<f64, ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter_with_ctx(ctx).await?;
        let resource = chain.account_resource(owner_address).await?;
        Ok(resource.resource_value(resource_type, amount)?)
    }

    fn required_balance_sun(transaction_fee_sun: i64, stake_amount_trx: i64) -> i64 {
        transaction_fee_sun.saturating_add(stake_amount_trx.saturating_mul(tron::consts::TRX_VALUE))
    }
}

#[cfg(test)]
mod tests {
    use super::ApiResourceDomain;
    use wallet_database::entities::{api_account::ApiAccountEntity, api_wallet::ApiWalletType};

    #[test]
    fn api_resource_amount_requires_positive_integer_trx() {
        assert_eq!(ApiResourceDomain::parse_amount_trx(1000).unwrap(), 1000);
        assert!(ApiResourceDomain::parse_amount_trx(0).is_err());
        assert!(ApiResourceDomain::parse_amount_trx(-1).is_err());
    }

    #[test]
    fn api_resource_stake_balance_check_uses_trx_unit() {
        assert_eq!(
            ApiResourceDomain::required_balance_sun(345, 2),
            2 * wallet_chain_interact::tron::consts::TRX_VALUE + 345
        );
    }

    #[test]
    fn api_withdraw_wallet_account_context_accepts_withdrawal_account() {
        let account = test_account(ApiWalletType::Withdrawal);
        let ctx = ApiResourceDomain::account_context_from_entity(account).unwrap();

        assert_eq!(ctx.uid, "withdraw-uid");
        assert_eq!(ctx.owner_address, "TWithdrawOwner");
    }

    #[test]
    fn api_withdraw_wallet_account_context_rejects_non_withdrawal_account() {
        let account = test_account(ApiWalletType::SubAccount);

        assert!(ApiResourceDomain::account_context_from_entity(account).is_err());
    }

    fn test_account(api_wallet_type: ApiWalletType) -> ApiAccountEntity {
        ApiAccountEntity {
            id: 1,
            account_id: 1,
            name: "withdraw account".to_string(),
            address: "TWithdrawOwner".to_string(),
            pubkey: Some("pubkey".to_string()),
            address_type: "".to_string(),
            wallet_address: "TWithdrawWallet".to_string(),
            uid: "withdraw-uid".to_string(),
            derivation_path: "m/44'/195'/0'/0/0".to_string(),
            derivation_path_index: 0,
            chain_code: "tron".to_string(),
            api_wallet_type,
            status: 1,
            is_init: 1,
            is_expand: 0,
            is_used: true,
            created_at: chrono::Utc::now(),
            updated_at: None,
        }
    }
}
