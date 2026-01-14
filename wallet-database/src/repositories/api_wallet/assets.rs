use crate::{
    DbPool,
    dao::api_assets::{ApiAssertSummeryEntity, ApiAssetsDao, SumResult},
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
        pool: &DbPool,
        assets: ApiCreateAssetsVo,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::upsert_assets(pool.as_ref(), assets).await
    }

    /// 批量插入或更新资产
    pub async fn upsert_assets_multi(
        pool: &DbPool,
        assets: Vec<ApiCreateAssetsVo>,
    ) -> Result<(), crate::Error> {
        if assets.is_empty() {
            return Ok(());
        }

        // 使用事务批量执行插入，确保数据一致性
        let mut tx = pool
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

    pub async fn update_balance(
        pool: &DbPool,
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
        pool: &DbPool,
        updates: Vec<(String, String, Option<String>, String)>, // (address, chain_code, token_address, balance)
    ) -> Result<(), crate::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        // 使用事务批量执行更新，减少数据库往返次数
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        ApiAssetsDao::batch_update_balance_in_tx(&mut tx, &updates).await?;

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn update_status(
        pool: &DbPool,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        status: u8,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::update_status(pool.as_ref(), chain_code, symbol, token_address, status).await
    }

    pub async fn find_by_id(
        pool: &DbPool,
        id: &AssetsIdVo<'_>,
    ) -> Result<Option<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::assets_by_id(pool.as_ref(), id).await?)
    }

    pub async fn list(
        pool: &DbPool,
        addr: Vec<String>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiAssetsEntity>, crate::Error> {
        Ok(ApiAssetsDao::list(pool.as_ref(), addr, chain_code).await?)
    }

    pub async fn get_chain_assets_by_address_chain_code_symbol(
        pool: &DbPool,
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
        pool: &DbPool,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), crate::Error> {
        ApiAssetsDao::delete_assets(pool.as_ref(), address, chain_code, token_address).await
    }

    pub async fn get_api_assets_by_address(
        pool: &DbPool,
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
        pool: &DbPool,
        address: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error> {
        ApiAssetsDao::assets_with_wallet_address_by_address(pool.as_ref(), address).await
    }

    pub async fn assets_with_wallet_address_by_token(
        pool: &DbPool,
        token: &[String],
    ) -> Result<Vec<AssetWithWalletAddress>, crate::Error> {
        ApiAssetsDao::assets_with_wallet_address_by_token(pool.as_ref(), token).await
    }

    pub async fn get_api_wallet_total_assets_v2(
        pool: &DbPool,
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
        pool: &DbPool,
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
}
