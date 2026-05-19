use crate::error::{
    business::api_wallet::{ApiWalletError, trans::TransError},
    service::ServiceError,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlatformResourceApplyAmounts {
    pub(crate) resource_amount: f64,
    pub(crate) native_token_amount: f64,
}

pub(crate) fn energy_shortfall_to_apply_amounts(
    amount: &str,
    energy_per_trx: f64,
) -> Result<PlatformResourceApplyAmounts, ServiceError> {
    let resource_amount: f64 = amount.parse().map_err(|e| {
        ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
            ApiWalletError::Trans(TransError::BuildWithdrawTransactionFailed(format!(
                "Invalid delegation amount: {}",
                e
            ))),
        ))
    })?;

    if resource_amount <= 0.0 {
        return Err(ServiceError::Parameter("delegation resource amount must be positive".into()));
    }
    if energy_per_trx <= 0.0 {
        return Err(ServiceError::Parameter("TRON energy price must be positive".into()));
    }

    Ok(PlatformResourceApplyAmounts {
        resource_amount,
        native_token_amount: resource_amount / energy_per_trx,
    })
}

#[cfg(test)]
mod tests {
    use super::energy_shortfall_to_apply_amounts;

    #[test]
    fn energy_shortfall_to_apply_amounts_uses_chain_energy_price() {
        let amounts = energy_shortfall_to_apply_amounts("800", 400.0).expect("amounts");

        assert_eq!(amounts.resource_amount, 800.0);
        assert_eq!(amounts.native_token_amount, 2.0);
    }

    #[test]
    fn energy_shortfall_to_apply_amounts_rejects_zero_energy_price() {
        assert!(energy_shortfall_to_apply_amounts("800", 0.0).is_err());
    }
}
