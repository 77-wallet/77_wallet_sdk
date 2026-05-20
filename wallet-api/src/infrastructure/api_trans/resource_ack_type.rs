use wallet_database::entities::{
    api_resource_delegation::{ApiResourceDelegationEntity, ApiResourceDelegationOperationType},
    api_trade_type::ApiTradeType,
};
use wallet_transport_backend::request::api_wallet::transaction::TransType;

/// Maps a resource-delegation fact to the backend ACK transaction type.
///
/// Original-order result facts are created on merchant wallets when the backend
/// pushes `AWM_CMD_RSC_RES` using the origin order number. Real resource task
/// facts are created on platform wallets and keep their resource-task type.
pub(crate) fn resource_delegation_ack_trans_type(
    resource_task: &ApiResourceDelegationEntity,
) -> TransType {
    let is_original_order_result = resource_task
        .origin_trade_no
        .as_deref()
        .map(|origin| origin == resource_task.resource_trade_no)
        .unwrap_or(false);

    match (resource_task.origin_trade_type, resource_task.operation_type, is_original_order_result)
    {
        (Some(x), _, true) if x == ApiTradeType::Collect as i64 => TransType::Col,
        (Some(x), _, true) if x == ApiTradeType::Withdraw as i64 => TransType::Wd,
        (Some(x), ApiResourceDelegationOperationType::Delegate, false)
            if x == ApiTradeType::Collect as i64 =>
        {
            TransType::ColRscDl
        }
        (Some(x), ApiResourceDelegationOperationType::Delegate, false)
            if x == ApiTradeType::Withdraw as i64 =>
        {
            TransType::WdRscDl
        }
        (Some(x), ApiResourceDelegationOperationType::Undelegate, false)
            if x == ApiTradeType::Collect as i64 =>
        {
            TransType::ColRscRc
        }
        (Some(x), ApiResourceDelegationOperationType::Undelegate, false)
            if x == ApiTradeType::Withdraw as i64 =>
        {
            TransType::WdRscRc
        }
        (origin_trade_type, operation_type, true) => {
            tracing::warn!(
                ?origin_trade_type,
                ?operation_type,
                resource_trade_no = %resource_task.resource_trade_no,
                "Unknown original-order resource result trade type, fallback to COL ack type"
            );
            TransType::Col
        }
        (origin_trade_type, operation_type, false) => {
            tracing::warn!(
                ?origin_trade_type,
                ?operation_type,
                resource_trade_no = %resource_task.resource_trade_no,
                "Unknown resource task origin trade type, fallback to COL_RSC_DL ack type"
            );
            TransType::ColRscDl
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use wallet_database::entities::{
        api_resource_delegation::{
            ApiResourceDelegationOperationType, ApiResourceDelegationSource,
            ApiResourceDelegationStatus,
        },
        api_resource_type::ApiResourceType,
    };

    fn base_resource_task(
        origin_trade_type: ApiTradeType,
        operation_type: ApiResourceDelegationOperationType,
    ) -> ApiResourceDelegationEntity {
        ApiResourceDelegationEntity {
            id: 1,
            uid: "u".to_string(),
            source: ApiResourceDelegationSource::Platform,
            operation_type,
            origin_trade_no: Some("ORIGIN".to_string()),
            origin_trade_type: Some(origin_trade_type as i64),
            resource_trade_no: "rsc_1".to_string(),
            chain_code: "tron".to_string(),
            owner_address: "owner".to_string(),
            receiver_address: "receiver".to_string(),
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

    #[test]
    fn resource_task_ack_type_uses_resource_operation_type() {
        let collect =
            base_resource_task(ApiTradeType::Collect, ApiResourceDelegationOperationType::Delegate);
        assert!(matches!(resource_delegation_ack_trans_type(&collect), TransType::ColRscDl));

        let withdraw = base_resource_task(
            ApiTradeType::Withdraw,
            ApiResourceDelegationOperationType::Delegate,
        );
        assert!(matches!(resource_delegation_ack_trans_type(&withdraw), TransType::WdRscDl));

        let collect_reclaim = base_resource_task(
            ApiTradeType::Collect,
            ApiResourceDelegationOperationType::Undelegate,
        );
        assert!(matches!(
            resource_delegation_ack_trans_type(&collect_reclaim),
            TransType::ColRscRc
        ));

        let withdraw_reclaim = base_resource_task(
            ApiTradeType::Withdraw,
            ApiResourceDelegationOperationType::Undelegate,
        );
        assert!(matches!(
            resource_delegation_ack_trans_type(&withdraw_reclaim),
            TransType::WdRscRc
        ));
    }

    #[test]
    fn original_order_result_ack_type_uses_origin_order_type() {
        let mut collect =
            base_resource_task(ApiTradeType::Collect, ApiResourceDelegationOperationType::Delegate);
        collect.resource_trade_no = "C_ORIGIN".to_string();
        collect.origin_trade_no = Some("C_ORIGIN".to_string());
        assert!(matches!(resource_delegation_ack_trans_type(&collect), TransType::Col));

        let mut withdraw = base_resource_task(
            ApiTradeType::Withdraw,
            ApiResourceDelegationOperationType::Delegate,
        );
        withdraw.resource_trade_no = "W_ORIGIN".to_string();
        withdraw.origin_trade_no = Some("W_ORIGIN".to_string());
        assert!(matches!(resource_delegation_ack_trans_type(&withdraw), TransType::Wd));
    }
}
