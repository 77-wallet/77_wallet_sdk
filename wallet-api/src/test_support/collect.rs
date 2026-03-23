use wallet_database::entities::api_collect::ApiCollectEntity;

use crate::{
    error::service::ServiceError, infrastructure::api_trans::collect::ShadowCollectWorker,
};

/// Test-facing wrapper around the collect shadow worker's fee check.
///
/// Keeping this in `test_support` lets integration tests exercise the real
/// workflow without exposing helper methods from the business worker itself.
pub async fn shadow_collect_check_fee(
    worker: &ShadowCollectWorker,
    req: &ApiCollectEntity,
) -> Result<bool, ServiceError> {
    worker.check_fee(req).await
}
