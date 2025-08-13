use super::ResourcesRepo;
use crate::{
    dao::bill::BillDao,
    entities::bill::{BillEntity, BillUpdateEntity, RecentBillListVo},
    pagination::Pagination,
    DbPool,
};
use sqlx::{Executor, Sqlite};

pub struct BillRepo {
    repo: ResourcesRepo,
}

impl BillRepo {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self {
            repo: ResourcesRepo::new(db_pool),
        }
    }
}

impl BillRepo {
    pub async fn last_bill(
        &self,
        chain_code: &str,
        address: &str,
    ) -> Result<Option<BillEntity>, crate::Error> {
        Ok(BillDao::last_bill(chain_code, address, &*self.repo.db_pool).await?)
    }

    // 获取交易
    pub async fn get_by_hash_and_owner(
        tx_hash: &str,
        owner: &str,
        pool: &DbPool,
    ) -> Result<BillEntity, crate::Error> {
        let bill = BillDao::get_by_hash_and_owner(pool.as_ref(), tx_hash, owner)
            .await?
            .ok_or(crate::Error::NotFound(format!(
                "bill not found,tx_hash = {} ,owenr = {}",
                tx_hash, owner,
            )))?;

        Ok(bill)
    }

    pub async fn find_by_id(id: &str, pool: &DbPool) -> Result<BillEntity, crate::Error> {
        let bill = BillDao::find_by_id(pool.as_ref(), id)
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
    ) -> Result<Vec<BillEntity>, crate::Error> {
        Ok(BillDao::lists_by_hashs(pool.as_ref(), owner, hashs).await?)
    }

    pub async fn recent_bill(
        symbol: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
        pool: DbPool,
    ) -> Result<Pagination<RecentBillListVo>, crate::Error> {
        let min_value = None;
        let lists =
            BillDao::recent_bill(symbol, addr, chain_code, min_value, page, page_size, pool)
                .await?;

        Ok(lists)
    }

    pub async fn update<'a, E>(
        transaction: &BillUpdateEntity,
        tx: E,
    ) -> Result<Option<BillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        Ok(BillDao::update(transaction, tx).await?)
    }

    pub async fn update_fail(tx_hash: &str, exec: &DbPool) -> Result<(), crate::Error> {
        BillDao::update_fail(tx_hash, exec.as_ref()).await?;

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
    ) -> Result<Pagination<BillEntity>, crate::Error> {
        let lists = BillDao::bill_lists(
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
        )
        .await?;
        Ok(lists)
    }
}

#[async_trait::async_trait]
pub trait BillRepoTrait: super::TransactionTrait {
    async fn bill_count(&mut self) -> Result<i64, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, BillDao::bill_count,)
    }
}
