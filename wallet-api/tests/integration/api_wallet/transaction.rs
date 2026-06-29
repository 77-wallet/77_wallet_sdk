use chrono::Utc;
use wallet_database::{
    ApiTransactionDbPool, SqliteContext,
    entities::{
        api_coin::ApiCoinData, api_trade_type::ApiTradeType, api_withdraw::ApiWithdrawStatus,
        asset_token_key::AssetTokenKey,
    },
    repositories::api_wallet::{coin::ApiCoinRepo, withdraw::ApiWithdrawRepo},
};

use crate::harness;

#[tokio::test]
async fn api_bill_detail_fails_when_current_api_resource_payload_uses_bandwidth_energy() {
    // Given an API withdraw row matching the device log shape.
    let env = harness::ensure_env().await;
    let api_wallet_pool = harness::open_api_wallet_pool(&env.db_dir).await;
    ensure_tron_main_coin(&api_wallet_pool).await;

    let tx_pool = open_api_transaction_pool(&env.db_dir).await;
    let owner = harness::next_tag("TWithdrawOwner");
    let tx_hash = harness::next_tag("tron-hash");
    ApiWithdrawRepo::upsert_api_withdraw(
        &tx_pool,
        "withdraw-uid",
        "withdraw-wallet",
        &owner,
        "TReceiver",
        "1",
        "",
        "tron",
        "TF17BgPaZYbz8oxbjhriubPDsA7ArKoLX3",
        "JST",
        &harness::next_tag("trade"),
        None,
        None,
        None,
        ApiTradeType::Withdraw,
        0,
        Some(tx_hash.clone()),
        ApiWithdrawStatus::ConfirmSuccessReport,
        ApiWithdrawStatus::ConfirmSuccessReport,
        r#"{"bandwidth":0,"energy":0}"#,
        "0.344",
        Some(Utc::now()),
        Some("68543373".to_string()),
    )
    .await
    .expect("seed api withdraw");

    // When the public API wallet detail entrypoint is called.
    let err = env
        .manager
        .api_bill_detail(&tx_hash, &owner)
        .await
        .expect_err("api_bill_detail should reproduce the resource parser failure");

    // Then it fails with the same missing net_used shape observed in the log.
    let err = err.to_string();
    assert!(err.contains("missing field `net_used`"), "{err}");
    assert!(err.contains(r#"{"bandwidth":0,"energy":0}"#), "{err}");
}

async fn ensure_tron_main_coin(pool: &wallet_database::ApiWalletDbPool) {
    if ApiCoinRepo::coin_by_chain_token_key_opt("tron", AssetTokenKey::Native, pool)
        .await
        .expect("query tron main coin")
        .is_some()
    {
        return;
    }

    let now = Utc::now();
    ApiCoinRepo::upsert_multi_coin(
        pool,
        vec![ApiCoinData::new(
            Some("TRON".to_string()),
            "TRX",
            "tron",
            AssetTokenKey::Native,
            Some("0".to_string()),
            Some("TRC20".to_string()),
            6,
            1,
            1,
            1,
            now,
            Some(now),
        )],
    )
    .await
    .expect("insert tron main coin");
}

async fn open_api_transaction_pool(db_dir: &std::path::Path) -> ApiTransactionDbPool {
    let sqlite = SqliteContext::new(&db_dir.to_string_lossy(), Some("api_transaction.db"))
        .await
        .expect("open api transaction sqlite");
    sqlite.into_transaction_db_pool().expect("api transaction pool")
}
