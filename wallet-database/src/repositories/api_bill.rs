use crate::dao::api_bill::ApiBillDao;
use crate::entities::api_bill::{ApiBillEntity, ApiBillUpdateEntity, ApiRecentBillListVo};
use crate::pagination::Pagination;
use crate::DbPool;

pub struct ApiBillRepo;

impl ApiBillRepo {
    pub async fn last_bill(
        pool: &DbPool,
        chain_code: &str,
        address: &str,
    ) -> Result<Option<ApiBillEntity>, crate::Error> {
        Ok(ApiBillDao::last_bill(chain_code, address, pool.as_ref()).await?)
    }

    // 获取交易
    pub async fn get_by_hash_and_owner(
        tx_hash: &str,
        owner: &str,
        pool: &DbPool,
    ) -> Result<ApiBillEntity, crate::Error> {
        let bill = ApiBillDao::get_by_hash_and_owner(pool.as_ref(), tx_hash, owner)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "bill not found,tx_hash = {} ,owenr = {}",
                tx_hash, owner,
            )))?;

        Ok(bill)
    }

    pub async fn find_by_id(id: &str, pool: &DbPool) -> Result<ApiBillEntity, crate::Error> {
        let bill = ApiBillDao::find_by_id(pool.as_ref(), id)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "bill not found,id = {}",
                id,
            )))?;

        Ok(bill)
    }

    pub async fn lists_by_hashs(
        owner: &str,
        hashs: Vec<String>,
        pool: &DbPool,
    ) -> Result<Vec<ApiBillEntity>, crate::Error> {
        Ok(ApiBillDao::lists_by_hashs(pool.as_ref(), owner, hashs).await?)
    }

    pub async fn recent_bill(
        symbol: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
        pool: DbPool,
    ) -> Result<Pagination<ApiRecentBillListVo>, crate::Error> {
        let min_value = None;
        let lists =
            ApiBillDao::recent_bill(symbol, addr, chain_code, min_value, page, page_size, pool.as_ref())
                .await?;

        Ok(lists)
    }

    pub async fn update<'a, E>(
        transaction: &ApiBillUpdateEntity,
        exec: &DbPool,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    {
        Ok(ApiBillDao::update(transaction, exec.as_ref()).await?)
    }

    pub async fn update_fail(tx_hash: &str, exec: &DbPool) -> Result<(), crate::Error> {
        ApiBillDao::update_fail(tx_hash, exec.as_ref()).await?;
        Ok(())
    }

    pub async fn bill_lists(
        addr: &[String],
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        min_value: Option<f64>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
        pool: &DbPool,
    ) -> Result<Pagination<ApiBillEntity>, crate::Error> {
        let lists = ApiBillDao::bill_lists(
            pool.as_ref(),
            addr,
            chain_code,
            symbol,
            is_multisig,
            min_value,
            start,
            end,
            transfer_type,
            page,
            page_size,
        ).await?;
        Ok(lists)
    }
}