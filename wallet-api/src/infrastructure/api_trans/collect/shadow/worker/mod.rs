mod collect_worker;
mod side_effect_worker;

use crate::{
    error::service::ServiceError,
    response_vo::{EthereumFeeDetails, FeeDetailsVo},
};
use alloy::primitives::U256;
use wallet_chain_interact::eth;
use wallet_utils::unit;

pub use collect_worker::{ShadowCollectCommand, ShadowCollectWorker};
pub use side_effect_worker::{SideEffectCommand, SideEffectWorker};

fn evm_propose_max_fee(fee: &str) -> Result<String, ServiceError> {
    let res: FeeDetailsVo<EthereumFeeDetails> = wallet_utils::serde_func::serde_from_str(fee)?;
    let propose = res.data.into_iter().find(|item| item.types == "propose").ok_or(
        crate::error::business::BusinessError::ApiWallet(
            crate::error::business::api_wallet::ApiWalletError::GasOracle,
        ),
    )?;

    let gas_limit = U256::from(propose.fee_setting.gas_limit);
    let max_fee_per_gas = unit::u256_from_str(&propose.fee_setting.max_fee_per_gas)?;
    unit::format_to_string(gas_limit * max_fee_per_gas, eth::consts::ETH_DECIMAL)
        .map_err(ServiceError::from)
}
