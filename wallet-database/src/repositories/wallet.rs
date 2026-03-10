use crate::{CoreDbPool, dao::wallet::WalletDao, entities::wallet::WalletEntity};
use sqlx::{Sqlite, Transaction};

pub struct WalletRepo;

impl WalletRepo {
    pub async fn detail(
        pool: CoreDbPool,
        address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        let wallet = WalletDao::detail(pool.as_ref(), address).await?;

        Ok(wallet)
    }

    pub async fn wallet_list(pool: CoreDbPool) -> Result<Vec<WalletEntity>, crate::Error> {
        let wallet = WalletDao::list(pool.as_ref()).await?;
        Ok(wallet)
    }

    pub async fn uid_list(pool: CoreDbPool) -> Result<Vec<(String,)>, crate::Error> {
        let wallet = WalletDao::uid_list(pool.as_ref()).await?;
        Ok(wallet)
    }

    pub async fn uid_list_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<(String,)>, crate::Error> {
        let wallet = WalletDao::uid_list(tx.as_mut()).await?;
        Ok(wallet)
    }

    pub async fn upsert_wallet(
        pool: CoreDbPool,
        address: &str,
        uid: &str,
        name: &str,
    ) -> Result<WalletEntity, crate::Error> {
        WalletDao::upsert(pool.as_ref(), address, uid, name, 1).await
    }

    pub async fn detail_all_status(
        pool: CoreDbPool,
        address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::detail_all_status(pool.as_ref(), address).await
    }

    pub async fn detail_all_status_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::detail_all_status(tx.as_mut(), address).await
    }

    pub async fn update_wallet_update_at(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::update_wallet_update_at(pool.as_ref(), wallet_address).await
    }

    pub async fn wallet_init(pool: CoreDbPool, uid: &str) -> Result<WalletEntity, crate::Error> {
        WalletDao::init(pool.as_ref(), uid).await
    }

    pub async fn edit_wallet_name(
        pool: CoreDbPool,
        wallet_address: &str,
        name: &str,
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::edit_wallet_name(pool.as_ref(), wallet_address, name).await
    }

    pub async fn wallet_latest(pool: CoreDbPool) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_latest(pool.as_ref()).await
    }

    pub async fn wallet_latest_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_latest(tx.as_mut()).await
    }

    pub async fn wallet_detail_by_name(
        pool: CoreDbPool,
        name: Option<String>,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_detail_by_wallet_name(pool.as_ref(), name).await
    }

    pub async fn wallet_detail_by_address(
        pool: CoreDbPool,
        address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_detail_by_wallet_address(pool.as_ref(), address).await
    }

    pub async fn wallet_detail_by_address_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        address: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_detail_by_wallet_address(tx.as_mut(), address).await
    }

    pub async fn wallet_detail_by_uid(
        pool: CoreDbPool,
        uid: &str,
    ) -> Result<Option<WalletEntity>, crate::Error> {
        WalletDao::wallet_detail_by_uid(pool.as_ref(), uid).await
    }

    pub async fn reset(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::reset_wallet(pool.as_ref(), wallet_address).await
    }

    pub async fn reset_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &str,
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::reset_wallet(tx.as_mut(), wallet_address).await
    }

    pub async fn reset_all_wallet(pool: CoreDbPool) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::reset_all_wallet(pool.as_ref()).await
    }

    pub async fn reset_all_wallet_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::reset_all_wallet(tx.as_mut()).await
    }

    pub async fn restart(
        pool: CoreDbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::restart_wallet(pool.as_ref(), wallet_addresses).await
    }

    pub async fn restart_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_addresses: &[&str],
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::restart_wallet(tx.as_mut(), wallet_addresses).await
    }

    pub async fn physical_delete(
        pool: CoreDbPool,
        wallet_address: &[&str],
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::delete_wallet(pool.as_ref(), wallet_address).await
    }

    pub async fn physical_delete_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &[&str],
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::delete_wallet(tx.as_mut(), wallet_address).await
    }

    pub async fn physical_delete_all(pool: CoreDbPool) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::delete_all_wallet(pool.as_ref()).await
    }

    pub async fn physical_delete_all_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<WalletEntity>, crate::Error> {
        WalletDao::delete_all_wallet(tx.as_mut()).await
    }
}
