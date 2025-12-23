use crate::{
    DbPool,
    dao::api_account::{ApiAccountDao, ApiAccountEntitySummer, ApiAccountSummeryEntity},
    entities::{
        account::AccountEntity,
        api_account::{
            AccountToWalletAddress, ApiAccountEntity, ApiAccountWalletMapping, CreateApiAccountVo,
        },
        api_wallet::ApiWalletType,
    },
};

pub struct ApiAccountRepo;

impl ApiAccountRepo {
    pub async fn find_one(
        pool: DbPool,
        address: &str,
        chain_code: &str,
        address_type: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_one(
            pool.as_ref(),
            address,
            chain_code,
            address_type,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn upsert(pool: DbPool, input: Vec<CreateApiAccountVo>) -> Result<(), crate::Error> {
        Ok(ApiAccountDao::upsert_multi(pool.as_ref(), input).await?)
    }

    pub async fn list_inited_indices(
        pool: DbPool,
        wallet_address: &str,
        chain_code: &str,
    ) -> Result<Vec<(i32,)>, crate::Error> {
        Ok(ApiAccountDao::list_inited_indices(pool.as_ref(), wallet_address, chain_code).await?)
    }

    pub async fn mark_as_used(
        pool: DbPool,
        wallet_address: &str,
        account_id: u32,
        chain_code: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::update_is_used(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
            true,
        )
        .await?)
    }

    pub async fn get_all_account_indices(
        pool: DbPool,
        uid: &str,
        chain_code: &str,
    ) -> Result<Vec<u32>, crate::Error> {
        Ok(ApiAccountDao::get_all_account_indices(pool.as_ref(), uid, chain_code).await?)
    }

    pub async fn init(
        pool: DbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::init(pool.as_ref(), address, chain_code).await?)
    }

    pub async fn expand(
        pool: DbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::expand(pool.as_ref(), address, chain_code).await?)
    }

    pub async fn delete(
        pool: DbPool,
        wallet_address: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::physical_delete(pool.as_ref(), wallet_address, account_id).await?)
    }

    pub async fn api_account_list(
        pool: DbPool,
        wallet_address: Option<String>,
        account_id: Option<u32>,
        chain_codes: Vec<String>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::api_account_list(pool.as_ref(), wallet_address, account_id, chain_codes)
            .await?)
    }

    pub async fn find_all_by_wallet_address_index(
        pool: DbPool,
        wallet_address: &str,
        chain_code: &str,
        account_id: u32,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_all_by_wallet_address_index(
            pool.as_ref(),
            wallet_address,
            chain_code,
            account_id,
        )
        .await?)
    }

    pub async fn has_account_id(
        pool: DbPool,
        wallet_address: &str,
        account_id: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<bool, crate::Error> {
        Ok(ApiAccountDao::has_account_id(
            pool.as_ref(),
            wallet_address,
            account_id,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn account_detail_by_max_id_and_wallet_address(
        pool: DbPool,
        wallet_address: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::account_detail_by_max_id_and_wallet_address(
            pool.as_ref(),
            wallet_address,
            api_wallet_type,
        )
        .await?)
    }

    pub async fn find_one_by_address_chain_code(
        address: &str,
        chain_code: &str,
        exec: DbPool,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_one_by_address_chain_code(address, chain_code, exec.as_ref())
            .await?)
    }

    pub async fn list_by_wallet_address(
        pool: DbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::lists_by_wallet_address(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
        )
        .await?)
    }

    pub async fn list(pool: DbPool) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        ApiAccountDao::account_list(pool.as_ref(), None, None, None, vec![], None).await
    }

    pub async fn list_by_wallet_address_account_id(
        pool: DbPool,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        ApiAccountDao::account_list(pool.as_ref(), wallet_address, None, None, vec![], account_id)
            .await
    }

    pub async fn account_wallet_mapping(
        pool: DbPool,
        api_wallet_type: Option<ApiWalletType>,
    ) -> Result<Vec<ApiAccountWalletMapping>, crate::Error> {
        ApiAccountDao::account_wallet_mapping(pool.as_ref(), api_wallet_type).await
    }

    pub async fn find_one_by_address(
        address: &str,
        exec: DbPool,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_one_by_address(address, exec.as_ref()).await?)
    }

    /// 批量查询账户（通过地址列表）
    pub async fn find_by_addresses(
        addresses: &[String],
        pool: DbPool,
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_by_addresses(addresses, pool.as_ref()).await?)
    }

    pub async fn find_one_by_wallet_address_account_id_chain_code(
        pool: DbPool,
        wallet_address: &str,
        account_id: u32,
        chain_code: &str,
    ) -> Result<Option<ApiAccountEntity>, crate::Error> {
        Ok(ApiAccountDao::find_one_by_wallet_address_account_id_chain_code(
            wallet_address,
            account_id,
            chain_code,
            pool.as_ref(),
        )
        .await?)
    }

    pub async fn edit_account_name(
        pool: DbPool,
        wallet_address: &str,
        account_id: u32,
        name: &str,
    ) -> Result<Vec<AccountEntity>, crate::Error> {
        Ok(ApiAccountDao::edit_account_name(pool.as_ref(), wallet_address, account_id, name)
            .await?)
    }

    pub async fn account_to_wallet(
        pool: DbPool,
    ) -> Result<Vec<AccountToWalletAddress>, crate::Error> {
        ApiAccountDao::account_to_wallet(pool.as_ref()).await
    }

    pub async fn physical_delete_all(
        pool: DbPool,
        wallet_addresses: &[&str],
    ) -> Result<Vec<ApiAccountEntity>, crate::Error> {
        ApiAccountDao::physical_delete_all(pool.as_ref(), wallet_addresses).await
    }

    pub async fn count_unique_account_ids(
        pool: DbPool,
        wallet_address: &str,
    ) -> Result<u32, crate::Error> {
        ApiAccountDao::count_unique_account_ids(pool.as_ref(), wallet_address).await
    }

    pub async fn lists_by_wallet_address_v2(
        pool: DbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiAccountSummeryEntity>, crate::Error> {
        ApiAccountDao::lists_by_wallet_address_v2(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
            page,
            page_size,
        )
        .await
    }

    pub async fn count_by_wallet_address_v2(
        pool: DbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<i64, crate::Error> {
        ApiAccountDao::count_by_wallet_address_v2(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
        )
        .await
    }

    pub async fn count_by_wallet_address_v3(
        pool: DbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<i64, crate::Error> {
        ApiAccountDao::count_by_wallet_address_v3(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
        )
        .await
    }

    pub async fn lists_by_wallet_address_v3(
        pool: DbPool,
        wallet_address: &str,
        account_ids: Vec<u32>,
        chain_code: Option<String>,
    ) -> Result<Vec<ApiAccountSummeryEntity>, crate::Error> {
        ApiAccountDao::lists_by_wallet_address_v3(
            pool.as_ref(),
            wallet_address,
            account_ids,
            chain_code,
        )
        .await
    }
    pub async fn lists_acc_by_wallet_address_v3(
        pool: DbPool,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<ApiAccountEntitySummer>, crate::Error> {
        ApiAccountDao::lists_acc_by_wallet_address_v3(
            pool.as_ref(),
            wallet_address,
            account_id,
            chain_code,
            page,
            page_size,
        )
        .await
    }
}
