use crate::{
    domain::{self, app::config::ConfigDomain, bill::BillDomain},
    response_vo::CoinCurrency,
};
use wallet_database::{
    dao::bill::BillDao,
    entities::{account::AccountEntity, bill::BillEntity},
    pagination::Pagination,
    repositories::{account::AccountRepoTrait, bill::BillRepoTrait, permission::PermissionRepo},
};

pub struct BillService<T: BillRepoTrait + AccountRepoTrait> {
    repo: T,
}

impl<T: BillRepoTrait + AccountRepoTrait> BillService<T> {
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn bill_lists(
        &mut self,
        root_addr: Option<String>,
        account_id: Option<u32>,
        addr: Option<String>,
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        filter_min_value: Option<bool>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Option<i64>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<BillEntity>, crate::ServiceError> {
        // TODO transaction don't need
        let adds = if let Some(addr) = addr {
            vec![addr]
        } else {
            let account = self
                .repo
                .account_list_by_wallet_address_and_account_id_and_chain_codes(
                    root_addr.as_deref(),
                    account_id,
                    Vec::new(),
                )
                .await?;

            let mut address = account
                .iter()
                .map(|item| item.address.clone())
                .collect::<Vec<String>>();

            // 兼容权限里面的地址
            let pool = crate::Context::get_global_sqlite_pool()?;
            let users = PermissionRepo::permission_by_users(&pool, &address).await?;

            for user in users {
                address.push(user.grantor_addr.clone());
            }
            address
        };

        // 过滤最小金额
        let min_value = match (symbol, filter_min_value) {
            (Some(symbol), Some(true)) => ConfigDomain::get_config_min_value(symbol).await?,
            _ => None,
        };

        let mut lists = self
            .repo
            .bill_lists(
                &adds,
                chain_code,
                symbol,
                is_multisig,
                min_value,
                start,
                end,
                transfer_type,
                page,
                page_size,
            )
            .await
            .map_err(crate::ServiceError::Database)?;

        lists
            .data
            .iter_mut()
            .for_each(|item| item.value = wallet_utils::unit::truncate_to_8_decimals(&item.value));

        Ok(lists)
    }

    pub async fn list_by_hashs(
        &mut self,
        owner: String,
        hashs: Vec<String>,
    ) -> Result<Vec<BillEntity>, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let res = BillDao::lists_by_hashs(pool.as_ref(), &owner, hashs).await?;

        Ok(res)
    }

    pub async fn sync_bill_by_address(
        &mut self,
        chain_code: &str,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        BillDomain::sync_bills(chain_code, address).await
    }

    pub async fn sync_bill_by_wallet_and_account(
        &mut self,
        wallet_address: String,
        account_id: u32,
    ) -> Result<(), crate::ServiceError> {
        // get all
        let executor = crate::Context::get_global_sqlite_pool()?;

        let accounts = AccountEntity::account_list(
            executor.as_ref(),
            Some(wallet_address.as_str()),
            None,
            None,
            vec![],
            Some(account_id),
        )
        .await?;

        for account in accounts.iter() {
            if let Err(e) = BillDomain::sync_bills(&account.chain_code, &account.address).await {
                tracing::warn!(
                    "[bill::sync_bill_by_wallet_and_account] chain_code:{},address {},fail {}",
                    account.chain_code,
                    account.address,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn coin_currency_price(
        &mut self,
        chain_code: String,
        symbol: String,
        token_address: Option<String>,
    ) -> Result<CoinCurrency, crate::ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token = domain::coin::TokenCurrencyGetter::get_currency(
            currency,
            &chain_code,
            &symbol,
            token_address,
        )
        .await?;

        Ok(CoinCurrency {
            currency: currency.to_string(),
            unit_price: token.currency_price,
        })
    }
}
