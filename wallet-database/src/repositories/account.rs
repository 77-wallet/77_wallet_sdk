use crate::{
    CoreDbPool,
    entities::account::{
        AccountEntity, AccountWalletMapping, AccountWithWalletEntity, CreateAccountVo,
    },
};
use sqlx::{Sqlite, Transaction};

pub struct AccountRepo;

impl AccountRepo {
    pub async fn list(pool: CoreDbPool) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::account_list_v2(pool.as_ref(), None, None, None, vec![], None).await
    }

    pub async fn get_all_account_indices(pool: CoreDbPool) -> Result<Vec<u32>, crate::Error> {
        AccountEntity::get_all_account_indices(pool.as_ref()).await
    }

    pub async fn detail_by_address_and_chain_code(
        pool: CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        AccountEntity::detail(pool.as_ref(), None, Some(address), None, Some(chain_code)).await
    }

    pub async fn detail_by_wallet_address_and_account_id_and_chain_code(
        pool: CoreDbPool,
        wallet_address: &str,
        account_id: u32,
        chain_code: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        AccountEntity::detail(
            pool.as_ref(),
            Some(wallet_address),
            None,
            Some(account_id),
            Some(chain_code),
        )
        .await
    }

    pub async fn account(
        pool: CoreDbPool,
        address: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        AccountEntity::detail(pool.as_ref(), None, Some(address), None, None).await
    }

    pub async fn upsert_multi_account(
        pool: CoreDbPool,
        input: Vec<CreateAccountVo>,
    ) -> Result<(), crate::Error> {
        AccountEntity::upsert_multi_account(pool.as_ref(), input).await
    }

    pub async fn account_wallet_mapping(
        pool: CoreDbPool,
    ) -> Result<Vec<AccountWalletMapping>, crate::Error> {
        AccountEntity::account_wallet_mapping(pool.as_ref()).await
    }

    pub async fn edit_account_name(
        pool: CoreDbPool,
        account_id: u32,
        wallet_address: &str,
        name: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::edit_account_name(pool.as_ref(), account_id, wallet_address, name).await
    }

    pub async fn account_detail_by_max_id_and_wallet_address(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        AccountEntity::account_detail_by_max_id_and_wallet_address(pool.as_ref(), wallet_address).await
    }

    pub async fn has_account_id(
        pool: CoreDbPool,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<bool, crate::Error> {
        AccountEntity::has_account_id(pool.as_ref(), wallet_address, account_id).await
    }

    pub async fn account_init(
        pool: CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::init(pool.as_ref(), address, chain_code).await
    }

    pub async fn get_account_list_by_wallet_address(
        pool: CoreDbPool,
        wallet_address: Option<&str>,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::account_list_v2(pool.as_ref(), wallet_address, None, None, vec![], None).await
    }

    pub async fn get_account_list_by_wallet_address_and_account_id(
        pool: CoreDbPool,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::account_list_v2(pool.as_ref(), wallet_address, None, None, vec![], account_id).await
    }

    pub async fn account_list_by_wallet_address_and_account_id_and_chain_codes(
        pool: CoreDbPool,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_codes: Vec<String>,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::account_list_v2(pool.as_ref(), wallet_address, None, None, chain_codes, account_id).await
    }

    pub async fn account_list_by_wallet_address_and_chain_code(
        pool: CoreDbPool,
        wallet_address: Option<&str>,
        chain_codes: Vec<String>,
        account_id: Option<u32>,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::account_list_v2(pool.as_ref(), wallet_address, None, None, chain_codes, account_id).await
    }

    pub async fn reset(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::reset_account(pool.as_ref(), wallet_address).await
    }

    pub async fn reset_tx(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::reset_account(tx.as_mut(), wallet_address).await
    }

    pub async fn reset_all_account(
        pool: CoreDbPool,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::reset_all_account(pool.as_ref()).await
    }

    pub async fn reset_all_account_tx(
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::reset_all_account(tx.as_mut()).await
    }

    pub async fn restart(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::restart_account(pool.as_ref(), wallet_address).await
    }

    pub async fn restart_tx(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::restart_account(tx.as_mut(), wallet_address).await
    }

    pub async fn physical_delete_all(
        pool: CoreDbPool,
        wallet_address: &[&str],
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::physical_delete_all(pool.as_ref(), wallet_address).await
    }

    pub async fn physical_delete_all_tx(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &[&str],
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::physical_delete_all(tx.as_mut(), wallet_address).await
    }

    pub async fn physical_delete(
        pool: CoreDbPool,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::physical_delete(pool.as_ref(), wallet_address, account_id).await
    }

    pub async fn physical_delete_tx(
        tx: &mut Transaction<'_, Sqlite>,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        AccountEntity::physical_delete(tx.as_mut(), wallet_address, account_id).await
    }

    pub async fn count_unique_account_ids(
        pool: CoreDbPool,
        wallet_address: &str,
    ) -> Result<u32, crate::Error> {
        AccountEntity::count_unique_account_ids(pool.as_ref(), wallet_address).await
    }

    pub async fn account_with_wallet(
        address: &str,
        chain_code: &str,
        pool: CoreDbPool,
    ) -> Result<AccountWithWalletEntity, crate::Error> {
        AccountEntity::account_with_wallet(address, chain_code, pool.as_ref()).await?.ok_or(
            crate::Error::NotFound(format!(
                "account not found: address: {}, chain_code: {}",
                address, chain_code
            )),
        )
    }

    pub async fn current_chain_address(
        address: String,
        account_id: u32,
        chain_code: &str,
        pool: CoreDbPool,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        Ok(AccountEntity::current_chain_address(address, account_id, chain_code, pool.as_ref())
            .await?)
    }
}
