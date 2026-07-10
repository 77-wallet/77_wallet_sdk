use chrono::{DateTime, Utc};

use crate::{error::service::ServiceError, infrastructure::chain_rpc_guard};

pub(crate) fn is_build_service_unavailable(err: &ServiceError) -> bool {
    chain_rpc_guard::ChainRpcErrorContext::from_display(err).is_service_unavailable()
}

pub(crate) fn build_retry_elapsed_secs(
    updated_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<i64> {
    updated_at.map(|updated_at| now.signed_duration_since(updated_at).num_seconds().max(0))
}

pub(crate) fn should_terminal_fail_build_service_unavailable(
    updated_at: Option<DateTime<Utc>>,
    err: &ServiceError,
    retry_window_secs: i64,
    now: DateTime<Utc>,
) -> bool {
    is_build_service_unavailable(err)
        && build_retry_elapsed_secs(updated_at, now)
            .is_some_and(|elapsed| elapsed >= retry_window_secs)
}

#[cfg(test)]
mod tests {
    use super::{
        build_retry_elapsed_secs, is_build_service_unavailable,
        should_terminal_fail_build_service_unavailable,
    };
    use chrono::{Duration, Utc};

    #[test]
    fn build_service_unavailable_detector_matches_node_503_message() {
        let err = crate::error::service::ServiceError::System(
            crate::error::system::SystemError::Internal(
                "Node response error: code=503, rpc=https://api.nileex.io/wallet/getaccount"
                    .to_string(),
            ),
        );

        assert!(is_build_service_unavailable(&err));
    }

    #[test]
    fn build_service_unavailable_terminal_failure_waits_for_retry_window() {
        let now = Utc::now();
        let err = crate::error::service::ServiceError::System(
            crate::error::system::SystemError::Internal(
                "Node response error: code=503, rpc=https://api.nileex.io/wallet/getaccount"
                    .to_string(),
            ),
        );

        assert_eq!(build_retry_elapsed_secs(Some(now - Duration::minutes(4)), now), Some(240));
        assert!(should_terminal_fail_build_service_unavailable(
            Some(now - Duration::minutes(4)),
            &err,
            180,
            now
        ));
        assert!(!should_terminal_fail_build_service_unavailable(
            Some(now - Duration::minutes(2)),
            &err,
            180,
            now
        ));
        assert!(!should_terminal_fail_build_service_unavailable(None, &err, 180, now));
    }
}
