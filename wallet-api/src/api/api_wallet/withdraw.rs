use crate::{
    api::ReturnType, manager::WalletManager, response_vo::api_wallet::withdraw::ApiWithdrawOrderVo,
    service::api_wallet::withdraw::WithdrawService,
};
use wallet_database::{
    entities::{api_withdraw::ApiWithdrawStatus, asset_token_key::AssetTokenKey},
    pagination::Pagination,
};

impl WalletManager {
    pub async fn list_api_withdraw_order(&self, uid: &str) -> ReturnType<Vec<ApiWithdrawOrderVo>> {
        WithdrawService::new(self.ctx).list_withdraw_order(uid).await
    }

    pub async fn page_api_withdraw_order_with_init_status(
        &self,
        uid: &str,
        init_status: u8,
        status: Vec<u8>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiWithdrawOrderVo>> {
        let s = status.iter().map(|it| ApiWithdrawStatus::try_from(it.clone()).unwrap()).collect();
        let init_status = ApiWithdrawStatus::try_from(init_status)?;
        WithdrawService::new(self.ctx)
            .page_withdraw_order_with_init_status(uid, init_status, s, page, page_size)
            .await
    }

    pub async fn page_api_withdraw_order(
        &self,
        uid: &str,
        status: Vec<u8>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiWithdrawOrderVo>> {
        let s = status.iter().map(|it| ApiWithdrawStatus::try_from(it.clone()).unwrap()).collect();
        WithdrawService::new(self.ctx).page_withdraw_order(uid, s, page, page_size).await
    }

    // 测试
    pub async fn api_withdrawal_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        validate: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
    ) -> ReturnType<()> {
        let token_key = AssetTokenKey::from_raw(token_address.as_deref());
        WithdrawService::new(self.ctx)
            .withdrawal_order(
                from, to, value, validate, chain_code, token_key, symbol, trade_no, trade_type,
                uid, 1,
            )
            .await
    }

    pub async fn sign_api_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new(self.ctx).sign_withdrawal_order(order_id).await
    }

    pub async fn reject_api_withdrawal_order(&self, order_id: &str) -> ReturnType<()> {
        WithdrawService::new(self.ctx).reject_withdrawal_order(order_id).await
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod test {
    use crate::test::env::get_manager;
    use anyhow::Result;
    use wallet_database::entities::api_withdraw::ApiWithdrawStatus;

    #[tokio::test]
    async fn test_reject_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let order_id = "W2020535510761119744";

        let res = wallet_manager.reject_api_withdrawal_order(order_id).await;
        tracing::info!("res: {res:?}");
        Ok(())
    }

    #[tokio::test]
    async fn test_page_api_withdraw_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let uid = "5bdb1b748bb617d6683f57565b1493cfa5f9e45f3086daf265ca2e0cd325c15e";
        let res = wallet_manager
            .page_api_withdraw_order(
                uid,
                vec![
                    ApiWithdrawStatus::AuditReject as u8,
                    ApiWithdrawStatus::SendingTxFailed as u8,
                ],
                0,
                10,
            )
            .await?;
        for e in &res.data {
            let res = serde_json::to_string(e).unwrap();
            tracing::info!("-------- {:?}", res);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_sign_api_withdrawal_order() -> Result<()> {
        wallet_utils::init_test_log();
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let order_id = "W2026501771206762496";
        let res = wallet_manager.sign_api_withdrawal_order(order_id).await;
        tracing::info!("sign_api_withdrawal_order result: {:?}", res);
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use chrono::{Duration, Utc};
    use wallet_database::entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
        asset_token_key::AssetTokenKey,
    };

    use crate::response_vo::api_wallet::{
        withdraw::ApiWithdrawOrderVo, withdraw_display::FailureReasonDisplay,
    };

    fn make_entity(
        created_at: chrono::DateTime<Utc>,
        audit_passed_at: Option<chrono::DateTime<Utc>>,
        audit_rejected_at: Option<chrono::DateTime<Utc>>,
    ) -> ApiWithdrawEntity {
        ApiWithdrawEntity {
            id: 0,
            name: "test".to_string(),
            uid: "test_uid".to_string(),
            from_addr: "from".to_string(),
            to_addr: "to".to_string(),
            value: "100".to_string(),
            validate: "validate".to_string(),
            chain_code: "TRX".to_string(),
            token_addr: AssetTokenKey::Native,
            symbol: "USDT".to_string(),
            trade_no: "T2024000000000000001".to_string(),
            trade_type: ApiTradeType::Withdraw,
            init_status: ApiWithdrawStatus::Init,
            status: ApiWithdrawStatus::Init,
            nonce: 0,
            tx_hash: None,
            raw_tx: None,
            resource_consume: "0".to_string(),
            transaction_fee: "0".to_string(),
            transaction_time: None,
            block_height: None,
            notes: None,
            post_tx_count: 0,
            post_confirm_tx_count: 0,
            err_code: None,
            err_msg: None,
            resource_check_at: None,
            resource_gate_released_at: None,
            resource_gate_result: None,
            resource_block_reason: None,
            resource_dependency_trade_no: None,
            resource_dependency_type: None,
            tx_ack_sent_at: None,
            building_at: None,
            last_broadcast_at: None,
            broadcast_uncertain_since_at: None,
            broadcast_uncertain_retry_count: 0,
            broadcast_uncertain_last_checked_at: None,
            broadcast_uncertain_reconciled_at: None,
            broadcast_uncertain_rebroadcast_count: 0,
            tx_res_ack_sent_at: None,
            tx_res_received_at: None,
            tx_exec_receipt_uploaded_at: None,
            finished_at: None,
            audit_passed_at,
            audit_rejected_at,
            audit_reason: None,
            chain_success_at: None,
            chain_failed_at: None,
            failure_stage: None,
            created_at,
            updated_at: None,
        }
    }

    #[test]
    fn test_apply_time_from_created_at() {
        let now = Utc::now();
        let entity = make_entity(now, None, None);
        let vo = ApiWithdrawOrderVo::from(entity);
        assert_eq!(vo.apply_time, now);
    }

    #[test]
    fn test_sign_time_from_audit_passed_at() {
        let now = Utc::now();
        let audit_time = now + Duration::hours(1);
        let entity = make_entity(now, Some(audit_time), None);
        let vo = ApiWithdrawOrderVo::from(entity);
        assert_eq!(vo.sign_time, Some(audit_time));
    }

    #[test]
    fn test_sign_time_from_audit_rejected_at() {
        let now = Utc::now();
        let reject_time = now + Duration::hours(2);
        let entity = make_entity(now, None, Some(reject_time));
        let vo = ApiWithdrawOrderVo::from(entity);
        assert_eq!(vo.sign_time, Some(reject_time));
    }

    #[test]
    fn test_sign_time_audit_passed_preferred_over_rejected() {
        let now = Utc::now();
        let pass_time = now + Duration::hours(1);
        let reject_time = now + Duration::hours(2);
        let entity = make_entity(now, Some(pass_time), Some(reject_time));
        let vo = ApiWithdrawOrderVo::from(entity);
        assert_eq!(vo.sign_time, Some(pass_time));
    }

    #[test]
    fn test_sign_time_none_when_no_audit() {
        let now = Utc::now();
        let entity = make_entity(now, None, None);
        let vo = ApiWithdrawOrderVo::from(entity);
        assert_eq!(vo.sign_time, None);
    }

    #[test]
    fn test_serde_camel_case_fields() {
        let now = Utc::now();
        let pass_time = now + Duration::hours(1);
        let entity = make_entity(now, Some(pass_time), None);
        let vo = ApiWithdrawOrderVo::from(entity);

        let json = serde_json::to_value(&vo).unwrap();
        assert!(json.get("applyTime").is_some(), "should have camelCase applyTime");
        assert!(json.get("signTime").is_some(), "should have camelCase signTime");
        assert!(json.get("apply_time").is_none(), "should NOT have snake_case apply_time");
        assert!(json.get("sign_time").is_none(), "should NOT have snake_case sign_time");
    }

    #[test]
    fn test_failure_reason_display() {
        let now = Utc::now();

        // Test Init status - should return None for non-failure status
        let entity = make_entity(now, None, None);
        let vo = ApiWithdrawOrderVo::from(entity);
        let json = serde_json::to_string_pretty(&vo).unwrap();
        println!("Init status:\n{}", json);
        assert_eq!(vo.failure_reason_display, None);

        // Test AuditReject status
        let mut entity = make_entity(now, None, None);
        entity.status = ApiWithdrawStatus::AuditReject;
        let vo = ApiWithdrawOrderVo::from(entity);
        let json = serde_json::to_string_pretty(&vo).unwrap();
        println!("AuditReject status:\n{}", json);
        assert_eq!(vo.failure_reason_display, Some(FailureReasonDisplay::AuditRejected));

        // Test Success status
        let mut entity = make_entity(now, None, None);
        entity.status = ApiWithdrawStatus::Success;
        let vo = ApiWithdrawOrderVo::from(entity);
        let json = serde_json::to_string_pretty(&vo).unwrap();
        println!("Success status:\n{}", json);
        assert_eq!(vo.failure_reason_display, None);
    }
}
