use super::{
    adapter::{AdapterGuard, RecordingEthAdapter, install_adapter_for_chain},
    db,
};
use crate::harness::{ensure_env, next_tag, open_api_wallet_pool};
use wallet_database::{
    ApiTransactionDbPool,
    entities::{
        api_collect::ApiCollectStatus, api_collect_strategy::ApiCollectStrategyEntity,
        api_collect_strategy_chain_config::ApiCollectStrategyChainConfigEntity,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, collect::ApiCollectRepo, collect_strategy::ApiCollectStrategyRepo,
        collect_strategy_chain_config::ApiCollectStrategyChainConfigRepo, nonce::ApiNonceRepo,
    },
};

pub(crate) struct CollectBuildScenario {
    ctx: &'static wallet_api::Context,
    pool: ApiTransactionDbPool,
    trade: String,
    from: String,
    pub adapter: RecordingEthAdapter,
    _guard: AdapterGuard,
}
impl CollectBuildScenario {
    pub async fn given() -> anyhow::Result<Self> {
        let env = ensure_env().await;
        let from = db::ensure_evm_transfer_fixture(env, "scroll").await?;
        let api_pool = open_api_wallet_pool(&env.db_dir).await;
        let account = ApiAccountRepo::find_one_by_address_chain_code(&from, "scroll", &api_pool)
            .await?
            .unwrap();
        let now = chrono::Utc::now();
        ApiCollectStrategyRepo::upsert(
            &api_pool,
            ApiCollectStrategyEntity {
                id: 0,
                uid: account.uid.clone(),
                threshold: 1,
                created_at: now,
                updated_at: None,
            },
        )
        .await?;
        let strategy = ApiCollectStrategyRepo::get_by_uid(&api_pool, &account.uid).await?.unwrap();
        let to = super::request::BNB_TO_ADDR;
        ApiCollectStrategyChainConfigRepo::upsert(
            &api_pool,
            ApiCollectStrategyChainConfigEntity {
                id: 0,
                strategy_id: strategy.id,
                chain_code: "scroll".into(),
                chain_address_type: None,
                normal_idx: None,
                normal_address: to.into(),
                risk_idx: None,
                risk_address: to.into(),
                created_at: now,
                updated_at: None,
            },
        )
        .await?;
        let pool = db::open_api_transaction_pool(&env.db_dir).await;
        let trade = next_tag("collect-main-build");
        let digest = wallet_utils::bytes_to_base64(&wallet_utils::md5_vec(
            &(from.clone() + "1" + env.manager.ctx().get_sn()),
        ));
        ApiCollectRepo::upsert_api_collect(
            &pool,
            &account.uid,
            "collect",
            &from,
            "",
            "1",
            &digest,
            "scroll",
            None,
            "ETH",
            &trade,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await?;
        ApiCollectRepo::set_order_ack_sent(&pool, &trade).await?;
        let adapter = RecordingEthAdapter::new();
        adapter.set_nonce_response(Some(5));
        adapter.fund_withdrawal();
        let guard = install_adapter_for_chain(&adapter, "scroll");
        Ok(Self { ctx: env.manager.ctx(), pool, trade, from, adapter, _guard: guard })
    }
    pub async fn when_build(&self) -> anyhow::Result<()> {
        wallet_api::testkit::collect::build_collect_candidate_for_test(self.ctx, &self.trade)
            .await?;
        Ok(())
    }
    pub async fn then_no_signed_reservation(&self) {
        let nonce = ApiNonceRepo::get_api_nonce(&self.pool, &self.from, "scroll").await.unwrap();
        assert!(nonce < 5, "failed or cancelled build consumed nonce {nonce}");
        assert!(
            ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, &self.trade)
                .await
                .unwrap()
                .raw_tx
                .is_none()
        );
    }
    pub async fn then_signed_once(&self) {
        let row =
            ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, &self.trade).await.unwrap();
        assert!(row.raw_tx.is_some());
        assert_eq!(row.nonce, 5);
        assert_eq!(ApiNonceRepo::get_api_nonce(&self.pool, &self.from, "scroll").await.unwrap(), 5);
    }
    pub async fn when_cancel_inflight(&self) {
        self.adapter.block_build(true);
        let build = self.when_build();
        tokio::pin!(build);
        tokio::select! {r=&mut build=>panic!("expected blocked build: {r:?}"),_=self.adapter.wait_for_first_entry()=>{}}
    }
    pub async fn given_stale_build_slot(&self) {
        sqlx::query(
            "UPDATE api_collect SET building_at=datetime('now','-1 hour') WHERE trade_no=?",
        )
        .bind(&self.trade)
        .execute(self.pool.write_ref())
        .await
        .unwrap();
        self.adapter.block_build(false);
    }
    pub async fn when_build_loses_reservation_race(&self) {
        self.adapter.block_build(true);
        let build = self.when_build();
        tokio::pin!(build);
        tokio::select! {r=&mut build=>panic!("expected blocked build: {r:?}"),_=self.adapter.wait_for_first_entry()=>{}}
        self.given_competing_reservation().await;
        self.adapter.release_first();
        assert!(build.await.is_err());
        self.then_no_raw().await;
    }
    pub async fn then_no_raw(&self) {
        let row =
            ApiCollectRepo::get_api_collect_by_trade_no(&self.pool, &self.trade).await.unwrap();
        assert!(row.raw_tx.is_none());
        assert!(row.tx_hash.is_none());
        assert_eq!(ApiNonceRepo::get_api_nonce(&self.pool, &self.from, "scroll").await.unwrap(), 5);
    }
    pub async fn given_competing_reservation(&self) {
        ApiNonceRepo::set_nonce_floor(&self.pool, &self.from, "scroll", 5).await.unwrap();
    }
}
