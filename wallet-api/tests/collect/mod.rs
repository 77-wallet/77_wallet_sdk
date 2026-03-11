use sqlx;
use tempfile::TempDir;
use wallet_database::{
    SqliteContext, entities::api_collect::ApiCollectStatus,
    repositories::api_wallet::collect::ApiCollectRepo,
};

struct TestFundsDb {
    _dir: TempDir,
    pool: wallet_database::ApiFundsDbPool,
}

impl TestFundsDb {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = SqliteContext::new(dir.path().to_string_lossy().as_ref(), Some("api_funds.db"))
            .await
            .expect("init api_funds.db");
        let pool = ctx.into_collect_db_pool().expect("collect pool");
        Self { _dir: dir, pool }
    }
}

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = TestFundsDb::new().await;
    let trade_no = "T_collect_blockhash_rebuild_refresh";

    ApiCollectRepo::upsert_api_collect(
        &db.pool,
        "uid",
        "collect",
        "from",
        "old-to",
        "1.12",
        "digest",
        "sol",
        Some("token".to_string()),
        "USDC",
        trade_no,
        2,
        ApiCollectStatus::Init,
        1,
    )
    .await
    .expect("insert collect");

    sqlx::query(
        r#"
        UPDATE api_collect
        SET raw_tx = $2,
            tx_hash = $3,
            status = $4,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE trade_no = $1
        "#,
    )
    .bind(trade_no)
    .bind("{\"stale\":true}")
    .bind("old-hash")
    .bind(ApiCollectStatus::SendingTx)
    .execute(db.pool.as_ref())
    .await
    .expect("set stale build facts");

    let invalidated = ApiCollectRepo::invalidate_raw_tx_for_rebuild(&db.pool, trade_no, None)
        .await
        .expect("invalidate raw tx for rebuild");
    assert_eq!(invalidated, 1);

    let after_invalidate = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load collect after invalidate");
    assert!(after_invalidate.raw_tx.is_none(), "stale raw_tx must be cleared");
    assert!(after_invalidate.tx_hash.is_none(), "stale tx_hash must be cleared");
    assert_eq!(
        after_invalidate.to_addr, "old-to",
        "rebuild invalidation must not invent a new execution address on its own"
    );

    ApiCollectRepo::update_api_collect_to_addr(&db.pool, trade_no, "new-to")
        .await
        .expect("persist rebuilt to_addr");

    let rebuilt = ApiCollectRepo::get_api_collect_by_trade_no(&db.pool, trade_no)
        .await
        .expect("load rebuilt collect");
    assert!(rebuilt.raw_tx.is_none(), "rebuild starts from cleared build facts");
    assert!(rebuilt.tx_hash.is_none(), "rebuild starts from cleared tx hash");
    assert_eq!(
        rebuilt.to_addr, "new-to",
        "next build must persist the latest strategy address before generating new raw_tx"
    );
}
