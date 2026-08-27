mod collect_worker;
mod side_effect_worker;

use crate::{
    error::service::ServiceError,
    response_vo::{EthereumFeeDetails, FeeDetailsVo},
};

pub use collect_worker::{ShadowCollectCommand, ShadowCollectWorker};
pub use side_effect_worker::{SideEffectCommand, SideEffectWorker};

fn evm_propose_max_fee(fee: &str) -> Result<String, ServiceError> {
    let res: FeeDetailsVo<EthereumFeeDetails> = wallet_utils::serde_func::serde_from_str(fee)?;
    let propose = res.data.into_iter().find(|item| item.types == "propose").ok_or(
        crate::error::business::BusinessError::ApiWallet(
            crate::error::business::api_wallet::ApiWalletError::GasOracle,
        ),
    )?;

    Ok(propose.max_fee.amount.to_string())
}
