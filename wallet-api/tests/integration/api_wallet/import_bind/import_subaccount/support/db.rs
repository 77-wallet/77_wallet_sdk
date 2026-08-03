use wallet_database::{
    entities::api_wallet::ApiWalletEntity, repositories::api_wallet::wallet::ApiWalletRepo,
};

use crate::harness::{self, find_wallet_by_uid, load_wallet_by_uid, open_api_wallet_pool};

pub(crate) async fn load_wallet(env: &harness::TestEnv, uid: &str) -> ApiWalletEntity {
    load_wallet_by_uid(env, uid).await
}

pub(crate) async fn find_wallet(env: &harness::TestEnv, uid: &str) -> Option<ApiWalletEntity> {
    find_wallet_by_uid(env, uid).await
}

pub(crate) async fn persisted_import_stage(env: &harness::TestEnv, uid: &str) -> Option<u8> {
    let pool = open_api_wallet_pool(&env.db_dir).await;
    ApiWalletRepo::find_by_uid(&pool, uid)
        .await
        .expect("query wallet by uid")
        .map(|wallet| wallet.import_stage)
}
