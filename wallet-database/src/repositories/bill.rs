use crate::{
    CoreDbPool,
    dao::bill::BillDao,
    entities::bill::{BillEntity, BillKind, BillUpdateEntity, RecentBillListVo},
    pagination::Pagination,
};
use sqlx::{Executor, Sqlite};

pub struct BillRepo;

impl BillRepo {
    pub fn new(_db_pool: crate::CoreDbPool) -> Self {
        Self
    }
}

impl BillRepo {
    pub async fn last_bill(
        pool: &CoreDbPool,
        chain_code: &str,
        address: &str,
    ) -> Result<Option<BillEntity>, crate::Error> {
        Ok(BillDao::last_bill(chain_code, address, pool.as_ref()).await?)
    }

    // 获取交易
    pub async fn get_by_hash_and_owner(
        tx_hash: &str,
        owner: &str,
        pool: &CoreDbPool,
    ) -> Result<BillEntity, crate::Error> {
        let bill = BillDao::get_by_hash_and_owner(pool.as_ref(), tx_hash, owner).await?.ok_or(
            crate::Error::NotFound(format!(
                "bill not found,tx_hash = {} ,owenr = {}",
                tx_hash, owner,
            )),
        )?;

        Ok(bill)
    }

    pub async fn get_by_hash_opt(
        hash: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<BillEntity>, crate::Error> {
        let bill = BillDao::get_one_by_hash(hash, pool.as_ref()).await?;

        Ok(bill)
    }

    pub async fn find_by_id(id: &str, pool: &CoreDbPool) -> Result<BillEntity, crate::Error> {
        let bill = BillDao::find_by_id(pool.as_ref(), id)
            .await?
            .ok_or(crate::Error::NotFound(format!("bill not found,id = {}", id,)))?;

        Ok(bill)
    }

    pub async fn lists_by_hashs(
        owner: &str,
        hashs: Vec<String>,
        pool: &CoreDbPool,
    ) -> Result<Vec<BillEntity>, crate::Error> {
        BillDao::lists_by_hashs(pool.as_ref(), owner, hashs).await
    }

    pub async fn recent_bill(
        token: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
        pool: CoreDbPool,
    ) -> Result<Pagination<RecentBillListVo>, crate::Error> {
        let min_value = None;
        let lists = BillDao::recent_bill(
            token,
            addr,
            chain_code,
            min_value,
            page,
            page_size,
            pool.into_inner(),
        )
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
        BillDao::update(transaction, tx).await
    }

    pub async fn update_fail(tx_hash: &str, exec: &CoreDbPool) -> Result<(), crate::Error> {
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
        pool: &CoreDbPool,
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

    pub async fn last_swap_bill(
        from: &str,
        chain_code: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<BillEntity>, crate::Error> {
        BillDao::last_swap_bill(pool.as_ref(), from, chain_code).await
    }

    pub async fn last_approve_bill(
        from: &str,
        to: &str,
        contract: &str,
        chain_code: &str,
        tx_kind: BillKind,
        pool: &CoreDbPool,
    ) -> Result<Option<BillEntity>, crate::Error> {
        BillDao::last_approve_bill(pool.as_ref(), from, to, contract, chain_code, tx_kind).await
    }

    pub async fn bill_count(pool: &CoreDbPool) -> Result<i64, crate::Error> {
        BillDao::bill_count(pool.as_ref()).await
    }
}

impl super::RepoCtx {
    pub async fn bill_count(&mut self) -> Result<i64, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, BillDao::bill_count,)
    }
}
