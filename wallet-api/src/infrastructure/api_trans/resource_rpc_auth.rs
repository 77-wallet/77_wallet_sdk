use crate::{
    context::Context, domain::api_wallet::trans::ApiTransDomain, error::service::ServiceError,
};

pub(crate) fn should_retry_after_rpc_auth_error(err: &ServiceError) -> bool {
    err.is_rpc_auth_unauthorized()
}

pub(crate) async fn refresh_and_prepare_retry(
    ctx: &Context,
    chain_code: &str,
    operation: &'static str,
    resource_trade_no: &str,
    err: &ServiceError,
) -> Result<(), ServiceError> {
    ApiTransDomain::refresh_rpc_auth_and_prepare_retry(
        ctx,
        chain_code,
        operation,
        Some(resource_trade_no),
        err,
    )
    .await
}

pub(crate) async fn refresh_and_prepare_retry_global(
    chain_code: &str,
    operation: &'static str,
    resource_trade_no: &str,
    err: &ServiceError,
) -> Result<(), ServiceError> {
    let ctx = crate::get_context()?;
    ApiTransDomain::refresh_rpc_auth_and_prepare_retry(
        ctx,
        chain_code,
        operation,
        Some(resource_trade_no),
        err,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::should_retry_after_rpc_auth_error;

    #[test]
    fn resource_rpc_auth_retry_matches_backend_401() {
        let err = crate::error::service::ServiceError::TransportBackend(
            wallet_transport_backend::Error::ApiBackend(401, Some("Unauthorized".to_string())),
        );

        assert!(should_retry_after_rpc_auth_error(&err));
    }
}
