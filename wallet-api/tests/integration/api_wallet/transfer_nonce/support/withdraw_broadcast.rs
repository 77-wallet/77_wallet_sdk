use super::{
    adapter::{AdapterGuard, RecordingEthAdapter, install_adapter},
    db,
};
use crate::harness::{TestEnv, ensure_env, next_tag};
use wallet_api::{domain::api_wallet::RawTx, testkit::withdraw::broadcast_withdraw_via_worker};
use wallet_database::{
    entities::{api_trade_type::ApiTradeType, api_withdraw::ApiWithdrawStatus},
    repositories::api_wallet::withdraw::ApiWithdrawRepo,
};

pub(crate) struct WithdrawBroadcastScenario {
    env: &'static TestEnv,
    pub(crate) adapter: RecordingEthAdapter,
    _guard: AdapterGuard,
    orders: Vec<String>,
    from: String,
}

impl WithdrawBroadcastScenario {
    pub(crate) async fn given_signed_batch() -> anyhow::Result<Self> {
        let env = ensure_env().await;
        let from = db::ensure_bnb_transfer_fixture(env).await?;
        let adapter = RecordingEthAdapter::new();
        adapter.set_nonce_response(Some(14));
        adapter.require_next_nonce();
        let guard = install_adapter(&adapter);
        let pool = db::open_api_transaction_pool(&env.db_dir).await;
        let mut orders = Vec::new();
        for nonce in 14_u64..=16 {
            let trade = next_tag("withdraw-broadcast-order");
            ApiWithdrawRepo::upsert_api_withdraw(
                &pool,
                "uid",
                "withdraw",
                &from,
                super::request::BNB_TO_ADDR,
                "1",
                "digest",
                "bnb",
                None,
                "BNB",
                &trade,
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
            .await?;
            let raw = serde_json::to_string(&RawTx::Evm(
                nonce.to_be_bytes().to_vec(),
                alloy::primitives::U256::ZERO,
            ))?;
            sqlx::query("UPDATE api_withdraws SET raw_tx=?, tx_hash=?, nonce=?, tx_ack_sent_at=CURRENT_TIMESTAMP, audit_passed_at=CURRENT_TIMESTAMP WHERE trade_no=?")
                .bind(raw).bind(adapter.tx_hash(nonce)).bind(nonce as i32).bind(&trade)
                .execute(pool.write_ref()).await?;
            orders.push(trade);
        }
        wallet_database::repositories::api_wallet::nonce::ApiNonceRepo::set_nonce_floor(
            &pool, &from, "bnb", 16,
        )
        .await?;
        Ok(Self { env, adapter, _guard: guard, orders, from })
    }

    pub(crate) async fn when_broadcast(&self, nonce: usize) -> anyhow::Result<()> {
        broadcast_withdraw_via_worker(self.env.manager.ctx(), &self.orders[nonce - 14]).await?;
        Ok(())
    }

    pub(crate) fn when_broadcast_starts(
        &self,
        nonce: usize,
    ) -> tokio::task::JoinHandle<Result<(), wallet_api::error::service::ServiceError>> {
        let ctx = self.env.manager.ctx();
        let trade = self.orders[nonce - 14].clone();
        tokio::spawn(async move { broadcast_withdraw_via_worker(ctx, &trade).await })
    }

    pub(crate) async fn then_order_is(&self, nonce: usize, sent: bool) -> anyhow::Result<()> {
        let row = ApiWithdrawRepo::get_api_withdraw_by_trade_no(
            &db::open_api_transaction_pool(&self.env.db_dir).await,
            &self.orders[nonce - 14],
            ApiTradeType::Withdraw,
        )
        .await?;
        assert_eq!(db::load_bnb_nonce_floor(self.env, &self.from).await?, 16);
        assert_eq!(row.nonce, nonce as i64);
        assert_eq!(row.tx_hash.as_deref(), Some(self.adapter.tx_hash(nonce as u64).as_str()));
        let raw: RawTx = serde_json::from_str(row.raw_tx.as_deref().unwrap())?;
        let RawTx::Evm(bytes, _) = raw else { panic!("EVM raw expected") };
        assert_eq!(bytes, (nonce as u64).to_be_bytes());
        assert_eq!(row.last_broadcast_at.is_some(), sent);
        assert!(row.err_code.is_none(), "waiting must not become ERR_6008: {:?}", row.err_msg);
        assert!(row.finished_at.is_none());
        assert!(row.tx_exec_receipt_uploaded_at.is_none());
        assert!(row.broadcast_uncertain_since_at.is_none());
        Ok(())
    }
}
