use std::time::{Duration, Instant};

use sqlx;
use wallet_api::testkit::withdraw::{
    scan_withdraw_intent_labels_for_trade_once,
    send_resource_result_ack_via_worker as send_withdraw_resource_result_ack_via_worker,
    upload_resource_tx_exec_receipt_via_worker as upload_withdraw_resource_tx_exec_receipt_via_worker,
};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, SqliteContext,
    entities::{
        api_resource_gate::{
            ApiResourceBlockReason, ApiResourceDependencyType, ApiResourceGateResult,
        },
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
    },
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

use crate::harness::{
    decrypt_captured_api_backend_body, ensure_worker_env, next_unique_id, open_api_wallet_pool,
    worker::WorkerTestEnv,
};

pub(super) struct WithdrawResourceGateFixture {
    trade_no: String,
    resource_trade_no: String,
}

impl WithdrawResourceGateFixture {
    pub(super) fn ack_payload_case(prefix: &str) -> Self {
        let id = next_unique_id();
        Self { trade_no: "W_ORIGIN_ACK".to_string(), resource_trade_no: format!("{prefix}_{id}") }
    }

    pub(super) fn origin_case(prefix: &str) -> Self {
        let trade_no = format!("{prefix}_{}", next_unique_id());
        Self { resource_trade_no: format!("DL_W_{trade_no}"), trade_no }
    }
}

pub(super) struct WithdrawResourceGateScenario {
    env: &'static WorkerTestEnv,
    tx_pool: ApiTransactionDbPool,
    core_pool: ApiWalletDbPool,
}

impl WithdrawResourceGateScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();

        let tx_pool = open_transaction_pool(env).await;
        let core_pool = open_api_wallet_pool(&env.db_dir).await;

        Self { env, tx_pool, core_pool }
    }

    pub(super) async fn given_resource_delegation_ready_for_ack(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_resource_delegation_ready_for_ack(&self.tx_pool, &fixture.resource_trade_no).await;
    }

    pub(super) async fn given_blocked_withdraw(&self, fixture: &WithdrawResourceGateFixture) {
        insert_withdraw(&self.tx_pool, &fixture.trade_no).await;
        mark_withdraw_blocked(&self.tx_pool, &fixture.trade_no, &fixture.resource_trade_no).await;
    }

    pub(super) async fn given_successful_withdraw_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Withdraw)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_release",
        )
        .await;
    }

    pub(super) async fn given_failed_withdraw_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_failed_resource_delegation(
            &self.tx_pool,
            &fixture.trade_no,
            ApiTradeType::Withdraw,
            &fixture.resource_trade_no,
        )
        .await;
    }

    pub(super) async fn given_resource_delegation_without_origin_trade(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            None,
            &fixture.resource_trade_no,
            "tx_hash_withdraw_no_origin",
        )
        .await;
    }

    pub(super) async fn given_collect_origin_resource_delegation(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        insert_successful_resource_delegation(
            &self.tx_pool,
            Some((&fixture.trade_no, ApiTradeType::Collect)),
            &fixture.resource_trade_no,
            "tx_hash_withdraw_wrong_origin",
        )
        .await;
    }

    pub(super) async fn when_resource_result_ack_is_sent(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        send_withdraw_resource_result_ack_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("send withdraw resource result ack");
    }

    pub(super) async fn when_resource_receipt_upload_is_sent(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        upload_withdraw_resource_tx_exec_receipt_via_worker(
            self.tx_pool.clone(),
            self.core_pool.clone(),
            &fixture.resource_trade_no,
        )
        .await
        .expect("upload withdraw resource tx exec receipt");
    }

    pub(super) async fn then_resource_result_ack_uses_withdraw_resource_type(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        let matched = self
            .wait_for_resource_ack_payload(&fixture.resource_trade_no, "TX_RES", "WD_RSC_DL")
            .await;

        let captured_requests = self.env.recorder.snapshot();
        let decoded_event_acks: Vec<_> = captured_requests
            .iter()
            .filter(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                )
            })
            .map(|req| decrypt_captured_api_backend_body(&req.body))
            .collect();

        assert!(
            matched,
            "withdraw resource result ack must use WD_RSC_DL; decoded event ack payloads: {:?}; captured requests: {:?}",
            decoded_event_acks, captured_requests
        );
    }

    pub(super) async fn then_origin_withdraw_gate_is_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
        expected_result: ApiResourceGateResult,
    ) {
        let withdraw = self.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_some());
        assert_eq!(withdraw.resource_gate_result, Some(expected_result));
    }

    pub(super) async fn then_origin_withdraw_gate_is_not_released(
        &self,
        fixture: &WithdrawResourceGateFixture,
    ) {
        let withdraw = self.load_withdraw(&fixture.trade_no).await;
        assert!(withdraw.resource_gate_released_at.is_none());
        assert!(withdraw.resource_gate_result.is_none());
    }

    pub(super) async fn then_withdraw_can_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().any(|label| label == "BuildTx"),
            "released withdraw should re-enter BuildTx"
        );
    }

    pub(super) async fn then_withdraw_cannot_build(&self, fixture: &WithdrawResourceGateFixture) {
        let labels = self.scan_withdraw_intent_labels(&fixture.trade_no).await;
        assert!(
            labels.iter().all(|label| label != "BuildTx"),
            "blocked withdraw should not be eligible for BuildTx before failed delegation bypass"
        );
    }

    async fn load_withdraw(&self, trade_no: &str) -> ApiWithdrawEntity {
        ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &self.tx_pool,
            trade_no,
            ApiTradeType::Withdraw,
        )
        .await
        .expect("load withdraw")
    }

    async fn scan_withdraw_intent_labels(&self, trade_no: &str) -> Vec<String> {
        scan_withdraw_intent_labels_for_trade_once(self.tx_pool.clone(), trade_no)
            .await
            .expect("scan withdraw labels")
    }

    async fn wait_for_resource_ack_payload(
        &self,
        resource_trade_no: &str,
        ack_type: &str,
        event_type: &str,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = self.env.recorder.snapshot().iter().any(|req| {
                req.path.contains(
                    wallet_transport_backend::consts::endpoint::api_wallet::TRANS_EVENT_ACK,
                ) && {
                    let payload = decrypt_captured_api_backend_body(&req.body);
                    payload["tradeNo"].as_str() == Some(resource_trade_no)
                        && payload["ackType"].as_str() == Some(ack_type)
                        && payload["type"].as_str() == Some(event_type)
                }
            });
            if found || Instant::now() >= deadline {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

async fn open_transaction_pool(env: &WorkerTestEnv) -> ApiTransactionDbPool {
    let tx_pool_ctx = SqliteContext::new(&env.db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    tx_pool_ctx.into_transaction_db_pool().expect("transaction pool")
}

async fn insert_withdraw(pool: &ApiTransactionDbPool, trade_no: &str) {
    ApiWithdrawRepo::upsert_api_withdraw(
        pool,
        "uid",
        "withdraw",
        "from",
        "to",
        "1.12",
        "digest",
        "tron",
        None,
        "TRX",
        trade_no,
        None,
        None,
        None,
        ApiTradeType::Withdraw,
        1,
        None,
        ApiWithdrawStatus::AuditPass,
        ApiWithdrawStatus::InitOrder,
        "",
        "",
        None,
        None,
    )
    .await
    .expect("insert withdraw");
}

async fn mark_withdraw_blocked(
    pool: &ApiTransactionDbPool,
    trade_no: &str,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        UPDATE api_withdraws
        SET tx_ack_sent_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            audit_passed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            resource_block_reason = ?,
            resource_dependency_trade_no = ?,
            resource_dependency_type = ?
        WHERE trade_no = ?
        "#,
    )
    .bind(ApiResourceBlockReason::NeedPlatformDelegate.as_i64())
    .bind(resource_trade_no)
    .bind(ApiResourceDependencyType::PlatformDelegate.as_i64())
    .bind(trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed blocked withdraw");
}

async fn insert_resource_delegation_ready_for_ack(
    pool: &ApiTransactionDbPool,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, 'W_ORIGIN_ACK', 1,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(resource_trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed withdraw delegation row for result ack");
}

async fn insert_successful_resource_delegation(
    pool: &ApiTransactionDbPool,
    origin: Option<(&str, ApiTradeType)>,
    resource_trade_no: &str,
    tx_hash: &str,
) {
    let (origin_trade_no, origin_trade_type) = origin
        .map_or((None, ApiTradeType::Withdraw), |(trade_no, trade_type)| {
            (Some(trade_no), trade_type)
        });

    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            tx_hash, tx_status, result_status, result_received_at, result_payload,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            ?, 'success', 1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), '{"status":true}',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(origin_trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .bind(tx_hash)
    .execute(pool.as_ref())
    .await
    .expect("seed successful resource delegation row");
}

async fn insert_failed_resource_delegation(
    pool: &ApiTransactionDbPool,
    origin_trade_no: &str,
    origin_trade_type: ApiTradeType,
    resource_trade_no: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO api_resource_delegation (
            uid, source, operation_type, origin_trade_no, origin_trade_type,
            resource_trade_no, chain_code, owner_address, receiver_address,
            resource_type, native_amount, amount, status,
            err_code, err_msg, tx_status,
            created_at, updated_at
        ) VALUES (
            'uid', 1, 1, ?, ?,
            ?, 'tron', 'owner', 'receiver',
            1, '2', '32000', 3,
            'delegate_failed', 'delegate failed', 'fail',
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        )
        "#,
    )
    .bind(origin_trade_no)
    .bind(origin_trade_type as i64)
    .bind(resource_trade_no)
    .execute(pool.as_ref())
    .await
    .expect("seed failed withdraw delegation row");
}
