use crate::{
    error::service::ServiceError,
    infrastructure::api_trans::collect_fee::shadow::ShadowFeeWorker,
    request::api_wallet::trans::ApiBaseTransferReq,
};

/// Test-facing wrapper for the Solana fee rent helper.
///
/// Keeping this in `test_support` allows integration tests to exercise the
/// behavior without embedding test code inside the deprecated fee worker module.
pub async fn bump_sol_native_transfer_value_for_rent(
    params: &mut ApiBaseTransferReq,
    symbol: &str,
    trade_no: &str,
) -> Result<(), ServiceError> {
    ShadowFeeWorker::bump_sol_native_transfer_value_for_rent(params, symbol, trade_no).await
}

/// Test-facing wrapper for the Solana rent error detector.
pub fn is_solana_recipient_rent_error(err: &ServiceError) -> bool {
    ShadowFeeWorker::is_solana_recipient_rent_error(err)
}
