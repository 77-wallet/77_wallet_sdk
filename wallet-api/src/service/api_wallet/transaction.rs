use crate::{
    domain::{
        self,
        api_wallet::account::ApiAccountDomain,
        app::config::ConfigDomain,
        bill::BillDomain,
        chain::{adapter::ChainAdapterFactory, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    request::transaction::{self},
    response_vo::transaction::{BillDetailVo, TransactionResult},
};
use wallet_database::{
    entities::bill::{BillEntity, BillKind, BillUpdateEntity, SyncBillEntity},
    pagination::Pagination,
    repositories::{
        api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo},
        bill::BillRepo,
        coin::CoinRepo,
    },
};
use wallet_utils::unit;

pub struct ApiTransService;

impl ApiTransService {
    pub async fn transfer(
        params: transaction::TransferReq,
        bill_kind: BillKind,
    ) -> Result<TransactionResult, crate::error::service::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&params.base.chain_code).await?;

        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        let tx_hash = ChainTransDomain::transfer(params, bill_kind, &adapter, private_key).await?;
        Ok(TransactionResult { tx_hash })
    }

    pub async fn bill_detail(
        tx_hash: &str,
        owner: &str,
    ) -> Result<BillDetailVo, crate::error::service::ServiceError> {
        let tx_hash = BillDomain::handle_hash(tx_hash);

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let mut bill = BillRepo::get_by_hash_and_owner(&tx_hash, owner, &pool).await?;
        bill.truncate_to_8_decimals();

        let main_coin = CoinRepo::main_coin(&bill.chain_code, &pool).await?;

        BillDetailVo::new(bill, main_coin.symbol, None)
    }

    pub async fn bill_lists(
        root_addr: Option<String>,
        account_id: Option<u32>,
        addr: Option<String>,
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        filter_min_value: Option<bool>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<BillEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let adds = if let Some(addr) = addr {
            vec![addr]
        } else {
            let chain_codes = if let Some(chain_code) = chain_code {
                vec![chain_code.to_string()]
            } else {
                vec![]
            };
            let account =
                ApiAccountRepo::api_account_list(&pool, root_addr, account_id, chain_codes).await?;

            account.iter().map(|item| item.address.clone()).collect::<Vec<String>>()
        };

        // 过滤最小金额
        let min_value = match (symbol, filter_min_value) {
            (Some(symbol), Some(true)) => ConfigDomain::get_config_min_value(symbol).await?,
            _ => None,
        };

        let mut lists = BillRepo::bill_lists(
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
            &pool,
        )
        .await?;

        lists.data.iter_mut().for_each(|item| item.truncate_to_8_decimals());

        Ok(lists)
    }

    pub async fn query_tx_result(
        req: Vec<String>,
    ) -> Result<Vec<BillEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let mut res = vec![];
        for id in req.iter() {
            match Self::sync_bill_info(id, pool.clone()).await {
                Ok(tx) => res.push(tx),
                Err(e) => {
                    tracing::warn!("sync bill err id = {},err = {}", id, e)
                }
            }
        }
        Ok(res)
    }

    async fn sync_bill_info(
        id: &str,
        pool: wallet_database::DbPool,
    ) -> Result<BillEntity, crate::error::service::ServiceError> {
        let transaction = BillRepo::find_by_id(id, &pool).await?;

        if transaction.status != wallet_database::entities::bill::BillStatus::Pending.to_i8() {
            return Ok(transaction);
        }

        // 不处理swap 类型的交易
        if transaction.tx_kind == BillKind::Swap.to_i8() {
            return Ok(transaction);
        }

        let sync_bill = match Self::get_tx_res(&transaction).await? {
            Some(tx_result) => tx_result,
            None => {
                // 处理交易是否失败的逻辑
                if transaction.is_failed() {
                    BillRepo::update_fail(&transaction.hash, &pool).await?;
                }
                return Ok(transaction);
            }
        };

        match Self::handle_pending_tx_status(&transaction, &sync_bill, &pool).await? {
            Some(tx) => Ok(tx),
            None => Ok(transaction),
        }
    }

    async fn handle_pending_tx_status(
        transaction: &BillEntity,
        sync_bill: &SyncBillEntity,
        pool: &wallet_database::DbPool,
    ) -> Result<Option<BillEntity>, crate::error::service::ServiceError> {
        // 1. 更新账单
        let tx_result = BillRepo::update(&sync_bill.tx_update, pool.as_ref()).await?;

        // 2. 更新余额
        ApiAssetsRepo::update_balance(
            pool,
            &transaction.owner,
            &transaction.chain_code,
            transaction.token.clone(),
            &sync_bill.balance,
        )
        .await?;

        Ok(tx_result)
    }

    async fn get_tx_res(
        transaction: &BillEntity,
    ) -> Result<Option<SyncBillEntity>, crate::error::service::ServiceError> {
        let adapter = domain::chain::adapter::ChainAdapterFactory::get_transaction_adapter(
            &transaction.chain_code,
        )
        .await?;

        let Some(tx_result) = adapter.query_tx_res(&transaction.hash).await? else {
            return Ok(None);
        };

        let token = transaction
            .token
            .as_ref()
            .filter(|token| !token.is_empty())
            .map(|token| token.to_string());

        // 查询余额
        let balance = adapter.balance(&transaction.owner, token.clone()).await?;

        let coin =
            CoinDomain::get_coin(&transaction.chain_code, &transaction.symbol, token).await?;

        let balance = unit::format_to_string(balance, coin.decimals)?;

        let tx_bill = BillUpdateEntity::new(
            tx_result.hash,
            tx_result.transaction_fee.to_string(),
            tx_result.transaction_time,
            tx_result.status,
            tx_result.block_height,
            tx_result.resource_consume,
        );

        let sync_bill = SyncBillEntity { tx_update: tx_bill, balance };

        Ok(Some(sync_bill))
    }
}
