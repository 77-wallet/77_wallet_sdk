use crate::{
    ApiWalletDbPool,
    dao::api_assets::{ApiAssertSummeryEntity, ApiAssetsDao, AssetBalanceEntity, SumResult},
    entities::{
        api_assets::{
            ApiAssetsEntity, ApiAssetsEntityWithAddressType, ApiCreateAssetsVo,
            AssetWithWalletAddress,
        },
        assets::AssetsIdVo,
    },
};

pub struct ApiAssetsRepo;

impl ApiAssetsRepo {
    pub async fn upsert_assets(
        pool: &ApiWalletDbPool,
        assets: ApiCreateAssetsVo,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    /// 批量插入或更新资产
    pub async fn upsert_assets_multi(
        pool: &ApiWalletDbPool,
        assets: Vec<ApiCreateAssetsVo>,
    ) -> Result<(), crate::Error> {
        if assets.is_empty() {
            return Ok(());
        }

        // 使用事务批量执行插入，确保数据一致性
        let mut tx = pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 获取事务的底层连接
        let conn = tx.as_mut();

        // 调用 DAO 层的批量插入方法
        ApiAssetsDao::upsert_assets_multi(conn, assets).await?;

        // 提交事务
        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    /// 批量插入或更新资产（仅用于“余额同步”场景）
    ///
    /// ON CONFLICT 时只更新 `balance`，避免被默认资产初始化覆盖。
    pub async fn upsert_assets_multi_update_balance(
        pool: &ApiWalletDbPool,
        assets: Vec<ApiCreateAssetsVo>,
    ) -> Result<(), crate::Error> {
        if assets.is_empty() {
            return Ok(());
        }

        let mut tx = pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        let conn = tx.as_mut();
        ApiAssetsDao::upsert_assets_multi_update_balance(conn, assets).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;
        Ok(())
    }

    pub async fn update_balance(
        pool: &ApiWalletDbPool,
        address: &str,
        chain_code: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_balance(pool.as_ref(), address, chain_code, token_address, balance)
            .await
    }

    /// 批量更新余额（使用事务批量执行，提升性能）
    pub async fn batch_update_balance(
        pool: &ApiWalletDbPool,
        updates: Vec<(String, String, Option<String>, String)>, // (address, chain_code, token_address, balance)
    ) -> Result<(), crate::Error> {
        if updates.is_empty() {
            return Ok(());
        }
        // 使用事务批量执行更新，减少数据库往返次数
        let mut tx = pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        ApiAssetsDao::batch_update_balance_in_tx(&mut tx, &updates).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn update_status(
        pool: &ApiWalletDbPool,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_status(pool.as_ref(), chain_code, symbol, token_address, status).await
    }

    pub async fn find_by_id(
        pool: &ApiWalletDbPool,
        id: &AssetsIdVo<'_>,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::assets_by_id(pool.as_ref(), id).await?)
    }

    pub async fn list(
        pool: &ApiWalletDbPool,
        addr: Vec<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::list(pool.as_ref(), addr, chain_code).await?)
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol(
        pool: &ApiWalletDbPool,
        address: Vec<String>,
        chain_code: Option<String>,
        symbol: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        ApiAssetsDao::get_chain_assets_by_address_chain_code_symbol(
            pool.as_ref(),
            address,
            chain_code,
            symbol,
            is_multisig,
        )
        .await
    }

    pub async fn delete_assets(
        pool: &ApiWalletDbPool,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::delete_assets(pool.as_ref(), address, chain_code, token_address).await
    }

    pub async fn get_api_assets_by_address(
        pool: &ApiWalletDbPool,
        address: Vec<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<ApiAssetsEntityWithAddressType>, crate::Error> {
        ApiAssetsDao::get_api_assets_by_address(
            pool.as_ref(),
            address,
            None,
            None,
            None,
            is_multisig,
        )
        .await
    }

    pub async fn assets_with_wallet_address_by_address(
        pool: &ApiWalletDbPool,
        address: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error> {
        ApiAssetsDao::assets_with_wallet_address_by_address(pool.as_ref(), address).await
    }

    pub async fn assets_with_wallet_address_by_token(
        pool: &ApiWalletDbPool,
        token: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error> {
        ApiAssetsDao::assets_with_wallet_address_by_token(pool.as_ref(), token).await
    }

    pub async fn get_api_wallet_total_assets_v2(
        pool: &ApiWalletDbPool,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<SumResult, crate::Error> {
        ApiAssetsDao::get_api_wallet_total_assets_v2(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
        )
        .await
    }

    pub async fn get_api_wallet_assets_v2(
        pool: &ApiWalletDbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
        hide_zero_balance: bool,
    ) -> Result<Vec<ApiAssertSummeryEntity>, crate::Error> {
        ApiAssetsDao::get_api_wallet_assets_v2(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
            hide_zero_balance,
        )
        .await
    }

    pub async fn get_api_wallet_total_assets_v3(
        pool: &ApiWalletDbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<Vec<AssetBalanceEntity>, crate::Error> {
        ApiAssetsDao::get_api_wallet_total_assets_v3(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::ApiAssetsRepo;
    use crate::{
        dao::api_assets::ApiAssetsDao,
        entities::{
            api_assets::ApiCreateAssetsVo,
            api_chain::{ApiChainCreateVo, NodeBindType},
            api_coin::ApiCoinData,
            assets::{AssetsId, AssetsIdVo},
        },
        repositories::{
            api_wallet::{chain::ApiChainRepo, coin::ApiCoinRepo},
            test_helper::setup_api_wallet_pool,
        },
    };
    use chrono::Utc;

    async fn seed_active_chain_and_coin(
        pool: &crate::ApiWalletDbPool,
        chain_code: &str,
        symbol: &str,
        token: Option<String>,
    ) {
        let protocols = vec!["evm".to_string()];
        let chain = ApiChainCreateVo::new(
            "Ethereum",
            chain_code,
            &protocols,
            NodeBindType::AutoLocal,
            "ETH",
        );
        ApiChainRepo::add(pool, chain).await.unwrap();

        let coin = ApiCoinData::new(
            Some(symbol.to_string()),
            symbol,
            chain_code,
            token,
            Some("1".to_string()),
            None,
            6,
            1,
            1,
            1,
            Utc::now(),
            None,
        );
        ApiCoinRepo::upsert_multi_coin(pool, vec![coin]).await.unwrap();
    }

    fn make_asset(address: &str, token: Option<String>, balance: &str) -> ApiCreateAssetsVo {
        let id =
            AssetsId::new(address, wallet_types::constant::chain_code::ETHEREUM, "USDT", token);
        ApiCreateAssetsVo::new(id, 6, None, 0).with_name("usdt").with_balance(balance)
    }

    #[tokio::test]
    async fn assets_repo_upsert_and_find_success() {
        let pool = setup_api_wallet_pool("wallet_db_api_assets_success").await;
        let address = "0xapi_assets_s_1";
        let chain_code = wallet_types::constant::chain_code::ETHEREUM;
        let token = Some("0xapi_assets_token_1".to_string());
        seed_active_chain_and_coin(&pool, chain_code, "USDT", token.clone()).await;

        ApiAssetsRepo::upsert_assets(&pool, make_asset(address, token.clone(), "12.5"))
            .await
            .unwrap();

        let id = AssetsIdVo::new(address, chain_code, token.clone());
        let got = ApiAssetsRepo::find_by_id(&pool, &id).await.unwrap().unwrap();
        assert_eq!(got.address, address);
        assert_eq!(got.chain_code, chain_code);
        assert_eq!(got.symbol, "USDT");
        assert_eq!(got.balance, "12.5");

        let rows =
            ApiAssetsRepo::list(&pool, vec![address.to_string()], Some(chain_code.to_string()))
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].address, address);
        assert_eq!(rows[0].balance, "12.5");
    }

    #[tokio::test]
    async fn assets_repo_missing_id_returns_none() {
        let pool = setup_api_wallet_pool("wallet_db_api_assets_edge").await;
        let id = AssetsIdVo::new(
            "0xapi_assets_missing",
            wallet_types::constant::chain_code::ETHEREUM,
            Some("0xapi_assets_missing_token".to_string()),
        );
        let got = ApiAssetsRepo::find_by_id(&pool, &id).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn assets_repo_tx_rollback_keeps_balance_unchanged() {
        let pool = setup_api_wallet_pool("wallet_db_api_assets_rollback").await;
        let address = "0xapi_assets_rb_1";
        let token = Some("0xapi_assets_rb_token_1".to_string());
        seed_active_chain_and_coin(
            &pool,
            wallet_types::constant::chain_code::ETHEREUM,
            "USDT",
            token.clone(),
        )
        .await;

        ApiAssetsRepo::upsert_assets(&pool, make_asset(address, token.clone(), "1")).await.unwrap();

        let mut tx = pool.as_ref().begin().await.unwrap();
        ApiAssetsDao::update_balance(
            tx.as_mut(),
            address,
            wallet_types::constant::chain_code::ETHEREUM,
            token.clone(),
            "99",
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();

        let id =
            AssetsIdVo::new(address, wallet_types::constant::chain_code::ETHEREUM, token.clone());
        let got = ApiAssetsRepo::find_by_id(&pool, &id).await.unwrap().unwrap();
        assert_eq!(got.balance, "1");

        let rows = ApiAssetsRepo::list(
            &pool,
            vec![address.to_string()],
            Some(wallet_types::constant::chain_code::ETHEREUM.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].balance, "1");
    }
}
