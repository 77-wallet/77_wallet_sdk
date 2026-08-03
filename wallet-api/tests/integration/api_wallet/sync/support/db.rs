use chrono::Utc;
use wallet_database::{
    entities::{
        api_account::CreateApiAccountVo,
        api_assets::{ApiAssetsEntity, ApiCreateAssetsVo},
        api_chain::{ApiChainCreateVo, NodeBindType},
        api_coin::ApiCoinData,
        api_wallet::ApiWalletType,
        asset_token_key::AssetTokenKey,
        assets::AssetsId,
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo, coin::ApiCoinRepo,
    },
};

use crate::harness::{TestEnv, open_api_wallet_pool, upsert_wallet};

use super::fixtures::{CHAIN_CODE, SyncAssetsFixture};

pub(crate) async fn prepare_wallet_fixture(
    env: &TestEnv,
    wallet_uid: &str,
    account_address: &str,
    token_address: AssetTokenKey,
    wallet_type: ApiWalletType,
) -> anyhow::Result<String> {
    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let now = Utc::now();

    ApiChainRepo::add(
        &api_pool,
        ApiChainCreateVo::new(
            "BNB Smart Chain",
            CHAIN_CODE,
            &["m/44'/60'/0'/0".to_string()],
            NodeBindType::AutoBackend,
            "BNB",
        ),
    )
    .await?;

    ApiCoinRepo::upsert_multi_coin(
        &api_pool,
        vec![ApiCoinData::new(
            Some("BNB Smart Chain".to_string()),
            "BNB",
            CHAIN_CODE,
            token_address.clone(),
            Some("0".to_string()),
            None,
            18,
            1,
            1,
            1,
            now,
            Some(now),
        )],
    )
    .await?;

    let wallet_address = upsert_wallet(&env.db_dir, "sn-sync", wallet_uid, wallet_type, None).await;

    let account = CreateApiAccountVo::new(
        1,
        account_address,
        "pubkey",
        &wallet_address,
        wallet_uid,
        "m/44'/60'/0'/0/0",
        0,
        CHAIN_CODE,
        "account",
        wallet_type,
    )
    .with_is_init(true);
    ApiAccountRepo::upsert_account_multi(&api_pool, vec![account]).await?;

    let asset = ApiCreateAssetsVo::new(
        AssetsId::new(account_address, CHAIN_CODE, token_address),
        "BNB",
        18,
        None,
        0,
    )
    .with_name("BNB")
    .with_balance("0");
    ApiAssetsRepo::upsert_assets_multi(&api_pool, vec![asset]).await?;

    Ok(wallet_address)
}

pub(crate) async fn load_native_asset(
    env: &TestEnv,
    fixture: &SyncAssetsFixture,
) -> anyhow::Result<ApiAssetsEntity> {
    let api_pool = open_api_wallet_pool(&env.db_dir).await;
    let saved = ApiAssetsRepo::find_by_id(
        &api_pool,
        &AssetsId::new(&fixture.account_address, CHAIN_CODE, AssetTokenKey::Native),
    )
    .await?
    .expect("asset should exist");

    Ok(saved)
}
