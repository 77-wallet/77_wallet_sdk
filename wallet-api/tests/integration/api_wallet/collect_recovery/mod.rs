mod support;

use serial_test::serial;

use support::{CollectRecoveryFixture, LocalCollectRecoveryDb, ShadowCollectRecoveryScenario};

#[tokio::test]
async fn collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr() {
    let db = LocalCollectRecoveryDb::new().await;
    let fixture = CollectRecoveryFixture::blockhash_rebuild();

    db.given_stale_blockhash_build(&fixture).await;

    let invalidated = db.when_raw_tx_is_invalidated_for_rebuild(&fixture).await;

    db.then_stale_build_facts_are_cleared(&fixture, invalidated).await;

    db.when_rebuilt_to_addr_is_persisted(&fixture, "new-to").await;

    db.then_rebuilt_to_addr_is_persisted(&fixture, "new-to").await;
}

#[tokio::test]
#[serial]
async fn collect_recover_queries_chain_before_any_expired_raw_rebuild_invalidation() {
    let mut scenario = ShadowCollectRecoveryScenario::new().await;
    let fixture = CollectRecoveryFixture::expired_tron_raw_probe();

    scenario.given_chain_probe_confirms_tx(&fixture);
    scenario.given_expired_raw_tx_collect(&fixture).await;

    scenario.when_recover_runs(&fixture).await;

    scenario.then_chain_was_queried_once();
    scenario.then_expired_raw_tx_is_confirmed_without_rebuild(&fixture).await;
}

#[tokio::test]
#[serial]
async fn collect_recover_backfills_missing_tx_hash_before_receipt_upload() {
    let mut scenario = ShadowCollectRecoveryScenario::new().await;
    let fixture = CollectRecoveryFixture::tron_backfill();

    scenario.given_recoverable_collect_with_tx_hash(&fixture).await;
    scenario.given_chain_query_clears_hash_then_confirms(&fixture);

    scenario.when_recover_runs(&fixture).await;

    scenario.then_tx_hash_is_backfilled_and_receipt_upload_needed(&fixture).await;
}

#[tokio::test]
async fn collect_scanner_recovers_broadcast_visible_pending_result() {
    let db = LocalCollectRecoveryDb::new().await;
    let fixture = CollectRecoveryFixture::broadcast_visible_pending();

    db.given_broadcast_visible_pending_collect(&fixture).await;

    let labels = db.when_collect_scanner_runs().await;

    db.then_scanner_emits_recover_only(labels);
    db.then_recoverable_row_stays_pending(&fixture).await;
}
