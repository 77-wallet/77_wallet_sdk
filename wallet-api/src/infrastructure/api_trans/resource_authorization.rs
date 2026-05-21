use wallet_chain_interact::tron::operations::stake::{DelegateArgs, UnDelegateArgs};
use wallet_database::{
    CoreDbPool,
    entities::api_resource_delegation::{ApiResourceDelegationEntity, ApiResourceDelegationMode},
    repositories::{api_wallet::account::ApiAccountRepo, permission::PermissionRepo},
};

use crate::error::service::ServiceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceDelegationSigner {
    pub(crate) signer_address: String,
    pub(crate) permission_id: Option<i64>,
}

pub(crate) async fn resolve_resource_delegation_signer(
    delegation: &ApiResourceDelegationEntity,
) -> Result<ResourceDelegationSigner, ServiceError> {
    match delegation.delegation_mode {
        ApiResourceDelegationMode::WithdrawAddress => Ok(ResourceDelegationSigner {
            signer_address: delegation.owner_address.clone(),
            permission_id: None,
        }),
        ApiResourceDelegationMode::AuthorizedAddress => {
            resolve_authorized_resource_signer(delegation).await
        }
    }
}

pub(crate) fn new_tron_delegate_args(
    owner_address: &str,
    receiver_address: &str,
    trx_amount: i64,
    resource: &str,
    permission_id: Option<i64>,
) -> wallet_chain_interact::Result<DelegateArgs> {
    let args = DelegateArgs::new(owner_address, receiver_address, trx_amount, resource)?;
    Ok(match permission_id {
        Some(permission_id) => args.with_permission_id(permission_id),
        None => args,
    })
}

pub(crate) fn new_tron_undelegate_args(
    owner_address: &str,
    receiver_address: &str,
    trx_amount: i64,
    resource: &str,
    permission_id: Option<i64>,
) -> wallet_chain_interact::Result<UnDelegateArgs> {
    UnDelegateArgs::new(owner_address, receiver_address, trx_amount, resource, permission_id)
}

async fn resolve_authorized_resource_signer(
    delegation: &ApiResourceDelegationEntity,
) -> Result<ResourceDelegationSigner, ServiceError> {
    let permission_id = delegation
        .permission_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ServiceError::Parameter(format!(
                "authorized resource delegation missing permissionId: trade_no={}",
                delegation.resource_trade_no
            ))
        })?;

    let ctx = crate::context::get_context()?;
    let core_pool = CoreDbPool::new(ctx.get_global_sqlite_pool()?);
    let permission = if let Ok(active_id) = permission_id.parse::<i64>() {
        PermissionRepo::permission_with_user(
            &core_pool,
            &delegation.owner_address,
            active_id,
            false,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?
    } else {
        let permission = PermissionRepo::find_option(&core_pool, permission_id)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        if let Some(permission) = permission {
            PermissionRepo::permission_with_user(
                &core_pool,
                &permission.grantor_addr,
                permission.active_id,
                false,
            )
            .await
            .map_err(|e| ServiceError::Database(e.into()))?
        } else {
            None
        }
    }
    .ok_or_else(|| {
        ServiceError::Parameter(format!(
            "authorized resource permission not found: owner={}, permissionId={}",
            delegation.owner_address, permission_id
        ))
    })?;

    if permission.permission.grantor_addr != delegation.owner_address {
        return Err(ServiceError::Parameter(format!(
            "authorized resource permission owner mismatch: trade_no={}, owner={}, permission_owner={}",
            delegation.resource_trade_no,
            delegation.owner_address,
            permission.permission.grantor_addr
        )));
    }

    let api_wallet_pool = ctx.api_wallet_pool()?;
    for user in permission.user {
        if ApiAccountRepo::find_one_by_address_chain_code(
            &user.address,
            &delegation.chain_code,
            &api_wallet_pool,
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?
        .is_some()
        {
            return Ok(ResourceDelegationSigner {
                signer_address: user.address,
                permission_id: Some(permission.permission.active_id),
            });
        }
    }

    Err(ServiceError::Parameter(format!(
        "authorized resource signer not found: owner={}, permissionId={}",
        delegation.owner_address, permission_id
    )))
}

#[cfg(test)]
mod tests {
    use super::resolve_resource_delegation_signer;
    use chrono::Utc;
    use wallet_database::entities::{
        api_resource_delegation::{
            ApiResourceDelegationEntity, ApiResourceDelegationMode,
            ApiResourceDelegationOperationType, ApiResourceDelegationSource,
            ApiResourceDelegationStatus,
        },
        api_resource_type::ApiResourceType,
        api_trade_type::ApiTradeType,
    };

    fn base_delegation() -> ApiResourceDelegationEntity {
        ApiResourceDelegationEntity {
            id: 1,
            uid: "uid".to_string(),
            source: ApiResourceDelegationSource::Platform,
            operation_type: ApiResourceDelegationOperationType::Delegate,
            origin_trade_no: None,
            origin_trade_type: Some(ApiTradeType::Collect as i64),
            resource_trade_no: "CD_missing_permission".to_string(),
            chain_code: "tron".to_string(),
            owner_address: "T_authorized_owner".to_string(),
            receiver_address: "T_receiver".to_string(),
            delegation_mode: ApiResourceDelegationMode::AuthorizedAddress,
            permission_id: None,
            resource_type: ApiResourceType::Energy,
            native_amount: "1".to_string(),
            amount: "100".to_string(),
            status: ApiResourceDelegationStatus::Pending,
            task_ack_sent_at: None,
            building_at: None,
            tx_hash: None,
            tx_status: None,
            tx_exec_receipt_uploaded_at: None,
            result_status: None,
            result_received_at: None,
            result_ack_sent_at: None,
            result_payload: None,
            fail_type: None,
            err_code: None,
            err_msg: None,
            recover_status: None,
            next_retry_at: None,
            retry_count: 0,
            created_at: Utc::now(),
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn authorized_resource_signer_requires_permission_id_before_context_lookup() {
        let err = resolve_resource_delegation_signer(&base_delegation())
            .await
            .expect_err("missing permissionId should fail");

        assert!(err.to_string().contains("missing permissionId"));
        assert!(!err.to_string().contains("Account not found"));
    }
}
