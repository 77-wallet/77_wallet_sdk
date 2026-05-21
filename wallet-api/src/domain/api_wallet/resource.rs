use crate::{
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, adapter::tx::RawTx},
        chain::adapter::ChainAdapterFactory,
    },
    error::{
        business::{
            BusinessError,
            api_wallet::{ApiWalletError, wallet::WalletError},
            chain::ChainError,
        },
        service::ServiceError,
    },
    request::api_wallet::resource::ApiResourceType,
};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::{
        self,
        operations::{
            TronTxOperation,
            stake::{FreezeBalanceArgs, UnFreezeBalanceArgs},
        },
    },
};
use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo},
};

pub(crate) struct ApiResourceDomain;

pub(crate) struct ApiResourceBroadcastOutcome {
    pub(crate) tx_hash: String,
    pub(crate) owner_address: String,
    pub(crate) raw_tx: String,
    pub(crate) transaction_fee: String,
}

impl ApiResourceDomain {
    pub(crate) async fn stake_withdraw_wallet_resource(
        ctx: &'static Context,
        withdraw_wallet_uid: &str,
        resource: ApiResourceType,
        frozen_balance: &str,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_address = Self::require_withdraw_wallet_address(ctx, withdraw_wallet_uid).await?;
        let frozen_balance_trx = Self::parse_amount_trx(frozen_balance)?;
        let resource = Self::tron_resource_name(resource);
        let args = FreezeBalanceArgs::new(&owner_address, resource, frozen_balance_trx, None)?;

        Self::execute_tron_resource_operation(owner_address, frozen_balance_trx, args).await
    }

    pub(crate) async fn unstake_withdraw_wallet_resource(
        ctx: &'static Context,
        withdraw_wallet_uid: &str,
        resource: ApiResourceType,
        unfreeze_balance: &str,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError> {
        let owner_address = Self::require_withdraw_wallet_address(ctx, withdraw_wallet_uid).await?;
        let unfreeze_balance_trx = Self::parse_amount_trx(unfreeze_balance)?;
        let resource = Self::tron_resource_name(resource);
        let args = UnFreezeBalanceArgs::new(&owner_address, resource, unfreeze_balance_trx, None)?;

        Self::execute_tron_resource_operation(owner_address, 0, args).await
    }

    async fn require_withdraw_wallet_address(
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

    fn parse_amount_trx(amount: &str) -> Result<i64, ServiceError> {
        let amount = amount.trim().parse::<i64>().map_err(|_| {
            ServiceError::Parameter(format!("invalid api wallet resource amount: {amount}"))
        })?;
        if amount <= 0 {
            return Err(ServiceError::Parameter(
                "api wallet resource amount must be positive".to_string(),
            ));
        }
        Ok(amount)
    }

    fn tron_resource_name(resource_type: ApiResourceType) -> &'static str {
        match resource_type {
            ApiResourceType::Energy => "energy",
            ApiResourceType::Bandwidth => "bandwidth",
        }
    }

    async fn execute_tron_resource_operation<T>(
        owner_address: String,
        stake_amount_trx: i64,
        args: impl TronTxOperation<T>,
    ) -> Result<ApiResourceBroadcastOutcome, ServiceError>
    where
        T: Send + 'static,
    {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
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

        let private_key = ApiAccountDomain::get_private_key(&owner_address, "tron").await?;
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
            tx_hash,
            owner_address,
            raw_tx: raw_tx_str,
            transaction_fee,
        })
    }

    fn required_balance_sun(transaction_fee_sun: i64, stake_amount_trx: i64) -> i64 {
        transaction_fee_sun.saturating_add(stake_amount_trx.saturating_mul(tron::consts::TRX_VALUE))
    }
}

#[cfg(test)]
mod tests {
    use super::ApiResourceDomain;
    use crate::request::api_wallet::resource::ApiResourceType;

    #[test]
    fn api_resource_amount_requires_positive_integer_trx() {
        assert_eq!(ApiResourceDomain::parse_amount_trx("1000").unwrap(), 1000);
        assert!(ApiResourceDomain::parse_amount_trx("0").is_err());
        assert!(ApiResourceDomain::parse_amount_trx("-1").is_err());
        assert!(ApiResourceDomain::parse_amount_trx("1.5").is_err());
    }

    #[test]
    fn api_resource_stake_balance_check_uses_trx_unit() {
        assert_eq!(
            ApiResourceDomain::required_balance_sun(345, 2),
            2 * wallet_chain_interact::tron::consts::TRX_VALUE + 345
        );
    }

    #[test]
    fn api_resource_type_maps_to_tron_resource_name() {
        assert_eq!(ApiResourceDomain::tron_resource_name(ApiResourceType::Energy), "energy");
        assert_eq!(ApiResourceDomain::tron_resource_name(ApiResourceType::Bandwidth), "bandwidth");
    }
}
