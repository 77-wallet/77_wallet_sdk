use super::{ApiBusinessNotificationService, AwmAddressInsufficientBody};
use rust_decimal::Decimal;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use wallet_database::{
    entities::{
        api_business_notification::NewApiBusinessNotification, asset_token_key::AssetTokenKey,
        assets::AssetsId,
    },
    repositories::api_business_notification::ApiBusinessNotificationRepo,
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

async fn reminder(
    ctx: &'static crate::context::Context,
    uid: &str,
    suffix: &str,
    address: &str,
    chain: &str,
    token: &str,
) -> String {
    let key = format!("{uid}-{suffix}");
    let body = AwmAddressInsufficientBody {
        remaining: Decimal::new(10, 0),
        threshold_value: Decimal::new(20, 0),
        token: token.into(),
        symbol: "fixture".into(),
        merchant_name: "fixture".into(),
        w_address_type: "1".into(),
        chain_code: chain.into(),
        address: address.into(),
    };
    let payload = serde_json::to_string(&body).unwrap();
    ApiBusinessNotificationRepo::upsert(
        &ctx.api_wallet_pool().unwrap(),
        NewApiBusinessNotification {
            id: &key,
            uid,
            kind: "AWM_ADDRESS_INSUFFICIENT",
            business_key: &key,
            payload: &payload,
        },
    )
    .await
    .unwrap();
    key
}

#[tokio::test]
async fn synced_balance_removes_only_matching_reminder_inside_balance_query_buffer() {
    // Arrange: cached low balances and independent asset/UID reminders.
    let ctx = crate::testkit::context::api_trans_test_ctx().await;
    let uid = format!("synced-buffer-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let other_uid = format!("{uid}-other");
    let target = reminder(ctx, &uid, "target", "address", "avalanche", "contract").await;
    reminder(ctx, &uid, "native", "address", "avalanche", "").await;
    reminder(ctx, &uid, "chain", "address", "base", "contract").await;
    reminder(ctx, &uid, "address", "other-address", "avalanche", "contract").await;
    reminder(ctx, &other_uid, "foreign", "address", "avalanche", "contract").await;
    let pool = ctx.api_wallet_pool().unwrap();
    let service = ApiBusinessNotificationService::new(ctx);
    let now = Instant::now();
    let calls = AtomicUsize::new(0);
    let first = service
        .unresolved_balance_notifications_for_uid_using(
            &uid,
            || now,
            |_, _, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                async { Ok(Decimal::new(10, 0)) }
            },
        )
        .await
        .unwrap();
    assert_eq!(first.len(), 4);

    // Act: consume the already persisted balance; do not query RPC again.
    service
        .resolve_from_persisted_balances(
            &uid,
            &[(
                AssetsId::new("address", "avalanche", AssetTokenKey::from_raw(Some("contract"))),
                "20".into(),
            )],
            &service.all_unresolved(&uid).await.unwrap(),
        )
        .await
        .unwrap();
    let next = service
        .unresolved_balance_notifications_for_uid_using(
            &uid,
            || now,
            |_, _, _| async { panic!("the buffer must not require another RPC after asset sync") },
        )
        .await
        .unwrap();

    // Assert: DB and query agree immediately; unrelated reminders/read state survive.
    assert_eq!(next.len(), 3);
    assert!(!next.iter().any(|item| item.id == target));
    assert_eq!(calls.load(Ordering::Relaxed), 4);
    let history = service.list(&uid, false, 0, 10).await.unwrap();
    assert_eq!(history.data.iter().filter(|item| item.resolved_at.is_some()).count(), 1);
    assert!(history.data.iter().all(|item| !item.is_read));
    assert_eq!(
        ApiBusinessNotificationRepo::list(&pool, &other_uid, true, 0, 10)
            .await
            .unwrap()
            .total_count,
        1
    );
}

#[tokio::test]
async fn synced_balance_keeps_reminder_for_insufficient_invalid_or_unpersisted_balance() {
    // Arrange.
    let ctx = crate::testkit::context::api_trans_test_ctx().await;
    let uid = format!("synced-failure-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    reminder(ctx, &uid, "target", "address", "avalanche", "contract").await;
    let service = ApiBusinessNotificationService::new(ctx);
    let id = AssetsId::new("address", "avalanche", AssetTokenKey::from_raw(Some("contract")));
    // Act: failed writes are excluded by the caller; invalid/below-threshold observations stay active.
    service
        .resolve_from_persisted_balances(&uid, &[], &service.all_unresolved(&uid).await.unwrap())
        .await
        .unwrap();
    for balance in ["19.999", "invalid"] {
        service
            .resolve_from_persisted_balances(
                &uid,
                &[(id.clone(), balance.into())],
                &service.all_unresolved(&uid).await.unwrap(),
            )
            .await
            .unwrap();
    }
    // Assert.
    let history = service.list(&uid, false, 0, 10).await.unwrap();
    assert_eq!(history.total_count, 1);
    assert!(history.data[0].resolved_at.is_none());
    assert!(!history.data[0].is_read);
}
