use super::*;
use crate::{
    entities::api_wallet::ApiWalletType, repositories::test_helper::setup_api_wallet_pool,
};

#[tokio::test]
async fn detail_scope_history_and_read_state() {
    // Arrange: eligible duplicate rows, subaccount, inactive, orphan and blank owners.
    let pool = setup_api_wallet_pool("notification_detail_scope").await;
    for (uid, address, kind, status) in [
        ("a", "a1", ApiWalletType::Withdrawal, 1),
        ("a", "a2", ApiWalletType::Withdrawal, 1),
        ("sub", "s1", ApiWalletType::SubAccount, 1),
        ("off", "o1", ApiWalletType::Withdrawal, 0),
        (" ", "blank", ApiWalletType::Withdrawal, 1),
    ] {
        crate::dao::api_wallet::ApiWalletDao::upsert(
            pool.write_ref(),
            address,
            uid,
            "fixture",
            b"",
            b"",
            status,
            kind,
            None,
            "fixture",
            0,
        )
        .await
        .unwrap();
    }
    for uid in ["a", "sub", "off", "orphan", " "] {
        ApiBusinessNotificationRepo::upsert(
            &pool,
            NewApiBusinessNotification {
                id: uid,
                uid,
                kind: "AWM_ADDRESS_INSUFFICIENT",
                business_key: uid,
                payload: "{}",
            },
        )
        .await
        .unwrap();
    }
    ApiBusinessNotificationRepo::mark_resolved(&pool, "a", "a", true).await.unwrap();
    let before = ApiBusinessNotificationRepo::find(&pool, "a", "a").await.unwrap().unwrap();
    // Act: detail reads resolved history without marking it read or changing timestamps.
    let detail = ApiBusinessNotificationRepo::find_for_withdrawal_wallets(&pool, "a")
        .await
        .unwrap()
        .unwrap();
    // Assert: single eligible row, exact id binding and unchanged durable record.
    assert_eq!(detail.id, "a");
    assert!(!detail.is_read);
    assert!(detail.resolved_at.is_some());
    assert_eq!(serde_json::to_value(before).unwrap(), serde_json::to_value(detail).unwrap());
    for id in ["sub", "off", "orphan", " ", "missing", "' OR 1=1 --"] {
        assert!(
            ApiBusinessNotificationRepo::find_for_withdrawal_wallets(&pool, id)
                .await
                .unwrap()
                .is_none()
        );
    }
    sqlx::query("UPDATE api_wallet SET status = 0 WHERE uid = 'a'")
        .execute(pool.write_ref())
        .await
        .unwrap();
    assert!(
        ApiBusinessNotificationRepo::find_for_withdrawal_wallets(&pool, "a")
            .await
            .unwrap()
            .is_none()
    );
    assert!(!ApiBusinessNotificationRepo::find(&pool, "a", "a").await.unwrap().unwrap().is_read);
}
