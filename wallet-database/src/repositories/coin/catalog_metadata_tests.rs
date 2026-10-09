use super::CoinRepo;
use crate::{
    dao::coin::CoinDao,
    entities::{asset_token_key::AssetTokenKey, coin::CoinData},
    repositories::test_helper::setup_core_pool,
};
use chrono::{DateTime, Utc};

fn timestamp(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

fn catalog_coin(default: u8, updated: &str) -> CoinData {
    CoinData::new(
        Some("USDT".into()),
        "USDT",
        "polygon",
        AssetTokenKey::from_raw(Some("0xc2132d05d31c914a87c6611c10748aeb04b58e8f")),
        Some("1".into()),
        Some("ERC20".into()),
        6,
        default,
        0,
        1,
        false,
        timestamp("2026-10-01T14:40:23Z"),
        timestamp(updated),
    )
}

#[tokio::test]
async fn catalog_insert_preserves_distinct_server_timestamps() {
    let pool = setup_core_pool("catalog_insert_timestamps").await;
    let input = catalog_coin(0, "2026-10-08T11:41:04Z");
    let token = input.token_address.clone();
    CoinRepo::upsert_multi_coin(&pool, vec![input]).await.unwrap();
    let actual = CoinRepo::coin_by_chain_token_key("polygon", token, &pool).await.unwrap();
    assert_eq!(actual.created_at, timestamp("2026-10-01T14:40:23Z"));
    assert_eq!(actual.updated_at, Some(timestamp("2026-10-08T11:41:04Z")));
    assert_eq!(actual.is_default, 0);
}

#[tokio::test]
async fn catalog_conflict_updates_default_and_server_update_time() {
    let pool = setup_core_pool("catalog_conflict_timestamps").await;
    CoinRepo::upsert_multi_coin(&pool, vec![catalog_coin(0, "2026-10-02T00:00:00Z")])
        .await
        .unwrap();
    let input = catalog_coin(1, "2026-10-08T11:41:04Z");
    let token = input.token_address.clone();
    CoinRepo::upsert_multi_coin(&pool, vec![input]).await.unwrap();
    let actual = CoinRepo::coin_by_chain_token_key("polygon", token, &pool).await.unwrap();
    assert_eq!(actual.is_default, 1);
    assert_eq!(actual.status, 1);
    assert_eq!(actual.created_at, timestamp("2026-10-01T14:40:23Z"));
    assert_eq!(actual.updated_at, Some(timestamp("2026-10-08T11:41:04Z")));
}

#[tokio::test]
async fn catalog_rollback_preserves_default_and_timestamps() {
    let pool = setup_core_pool("catalog_rollback_timestamps").await;
    let input = catalog_coin(0, "2026-10-02T00:00:00Z");
    let token = input.token_address.clone();
    CoinRepo::upsert_multi_coin(&pool, vec![input]).await.unwrap();
    let before = CoinRepo::coin_by_chain_token_key("polygon", token.clone(), &pool).await.unwrap();
    let mut tx = pool.write_ref().begin().await.unwrap();
    CoinDao::upsert_multi_coin(tx.as_mut(), vec![catalog_coin(1, "2026-10-08T11:41:04Z")])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let after = CoinRepo::coin_by_chain_token_key("polygon", token, &pool).await.unwrap();
    assert_eq!(after.is_default, before.is_default);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);

    // A failed attempt must remain replayable; no partial timestamp acts as a cursor.
    CoinRepo::upsert_multi_coin(&pool, vec![catalog_coin(1, "2026-10-08T11:41:04Z")])
        .await
        .unwrap();
    let retried = CoinRepo::coin_by_chain_token_key(
        "polygon",
        catalog_coin(1, "2026-10-08T11:41:04Z").token_address,
        &pool,
    )
    .await
    .unwrap();
    assert_eq!(retried.is_default, 1);
    assert_eq!(retried.updated_at, Some(timestamp("2026-10-08T11:41:04Z")));
}

#[tokio::test]
async fn catalog_replay_repairs_legacy_timestamp_without_duplicate_rows() {
    let pool = setup_core_pool("catalog_legacy_replay").await;
    let legacy = catalog_coin(0, "2026-10-01T14:40:23Z");
    let token = legacy.token_address.clone();
    CoinRepo::upsert_multi_coin(&pool, vec![legacy]).await.unwrap();
    let before = CoinRepo::coin_by_chain_token_key("polygon", token.clone(), &pool).await.unwrap();
    assert_eq!(before.updated_at, Some(before.created_at));
    let count = CoinRepo::coin_count(&pool).await.unwrap();

    // Receiving the old-created row again repairs its metadata; repeated delivery is idempotent.
    for _ in 0..2 {
        CoinRepo::upsert_multi_coin(&pool, vec![catalog_coin(1, "2026-10-08T11:41:04Z")])
            .await
            .unwrap();
    }
    let actual = CoinRepo::coin_by_chain_token_key("polygon", token, &pool).await.unwrap();
    assert_eq!(CoinRepo::coin_count(&pool).await.unwrap(), count);
    assert_eq!(actual.created_at, before.created_at);
    assert_eq!(actual.updated_at, Some(timestamp("2026-10-08T11:41:04Z")));
    assert_eq!(actual.is_default, 1);
}
