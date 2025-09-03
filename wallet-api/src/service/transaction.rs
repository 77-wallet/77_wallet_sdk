use crate::{
    domain,
    domain::{
        bill::BillDomain,
        chain::{adapter::ChainAdapterFactory, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    request::transaction::{self},
    response_vo::{
        self,
        account::Balance,
        transaction::{BillDetailVo, TransactionResult},
    },
};
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use wallet_chain_interact::BillResourceConsume;
use wallet_database::{
    dao::{multisig_account::MultisigAccountDaoV1, multisig_queue::MultisigQueueDaoV1},
    entities,
    entities::{
        assets::{AssetsEntity, AssetsId},
        bill::{
            BillEntity, BillKind, BillStatus, BillUpdateEntity, RecentBillListVo, SyncBillEntity,
        },
        coin::CoinEntity,
        multisig_account::{MultisigAccountPayStatus, MultisigAccountStatus},
        multisig_queue::{MemberSignedResult, MultisigQueueStatus},
    },
    pagination::Pagination,
    repositories::{
        account::AccountRepo, address_book::AddressBookRepo, bill::BillRepo, coin::CoinRepo,
        multisig_queue::MultisigQueueRepo,
    },
};
use wallet_utils::unit;

pub struct TransactionService;

impl TransactionService {
    // 本币的余额
    pub async fn chain_balance(
        address: &str,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
    ) -> Result<Balance, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        let pool = crate::Context::get_global_sqlite_pool()?;
        let coin = CoinRepo::coin_by_symbol_chain(chain_code, symbol, token_address.clone(), &pool)
            .await?;

        let balance = adapter.balance(address, token_address).await?;
        let format_balance = unit::format_to_string(balance, coin.decimals)?;

        let balance = Balance {
            balance: format_balance.clone(),
            decimals: coin.decimals,
            original_balance: balance.to_string(),
        };

        ChainTransDomain::update_balance(
            address,
            chain_code,
            symbol,
            coin.token_address,
            &format_balance,
        )
        .await?;

        Ok(balance)
    }

    /// 计算交易的手续费
    pub async fn transaction_fee(
        mut params: transaction::BaseTransferReq,
    ) -> Result<response_vo::EstimateFeeResp, crate::ServiceError> {
        let coin =
            CoinDomain::get_coin(&params.chain_code, &params.symbol, params.token_address.clone())
                .await?;

        params.with_decimals(coin.decimals);
        params.with_token(coin.token_address());

        let main_coin = ChainTransDomain::main_coin(&params.chain_code).await?;

        let adapter = ChainAdapterFactory::get_transaction_adapter(&params.chain_code).await?;
        let fee = adapter.estimate_fee(params, main_coin.symbol.as_str()).await?;

        let fee_resp =
            response_vo::EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), fee);
        Ok(fee_resp)
    }

    pub async fn transfer(
        params: transaction::TransferReq,
        bill_kind: BillKind,
    ) -> Result<TransactionResult, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&params.base.chain_code).await?;

        let tx_hash = ChainTransDomain::transfer(params, bill_kind, &adapter).await?;
        Ok(TransactionResult { tx_hash })
    }

    async fn handle_queue_member(
        bill: &BillEntity,
        pool: Arc<Pool<Sqlite>>,
    ) -> Option<Vec<MemberSignedResult>> {
        if !bill.signer.is_empty() {
            let signer = bill.signer.split(",").map(|s| s.to_string()).collect::<Vec<String>>();

            let mut result = vec![];
            for address in signer.iter() {
                let book = AddressBookRepo::find_by_address_chain(&pool, address, &bill.chain_code)
                    .await
                    .ok()
                    .flatten();
                let name = if let Some(book) = book { book.name } else { String::new() };

                let member = MemberSignedResult::new(&name, address, 0, 1);
                result.push(member);
            }
            return Some(result);
        }

        if bill.transfer_type != 1 || bill.queue_id.is_empty() {
            return None;
        }

        // 获取队列信息
        let queue = match domain::multisig::MultisigDomain::queue_by_id(&bill.queue_id, &pool).await
        {
            Ok(queue) => queue,
            Err(_) => return None,
        };

        (MultisigQueueRepo::signed_result(
            &queue.id,
            &queue.account_id,
            &queue.permission_id,
            pool.clone(),
        )
        .await)
            .ok()
    }

    pub async fn bill_detail(
        tx_hash: &str,
        owner: &str,
    ) -> Result<BillDetailVo, crate::ServiceError> {
        let tx_hash = BillDomain::handle_hash(tx_hash);

        let pool = crate::Context::get_global_sqlite_pool()?;

        let mut bill = BillRepo::get_by_hash_and_owner(&tx_hash, owner, &pool).await?;
        bill.truncate_to_8_decimals();

        let sign = Self::handle_queue_member(&bill, pool.clone()).await;

        let main_coin = CoinEntity::main_coin(&bill.chain_code, pool.as_ref()).await?.ok_or(
            crate::BusinessError::Coin(crate::CoinError::NotFound(format!(
                "chain = {}",
                bill.chain_code
            ))),
        )?;

        let resource_consume = if !bill.resource_consume.is_empty() && bill.resource_consume != "0"
        {
            Some(BillResourceConsume::from_json_str(&bill.resource_consume)?)
        } else {
            None
        };

        let mut res = BillDetailVo {
            bill,
            resource_consume,
            signature: sign,
            fee_symbol: main_coin.symbol,
            wallet_name: "".to_string(),
            account_name: "".to_string(),
        };
        if !res.bill.to_addr.is_empty() {
            // 根据地址和链获取钱包名称
            let account =
                AccountRepo::account_with_wallet(&res.bill.to_addr, &res.bill.chain_code, &pool)
                    .await;
            if let Ok(account) = account {
                res.wallet_name = account.wallet_name;
                res.account_name = account.name;
            }
        }

        Ok(res)
    }

    pub async fn recent_bill(
        token: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<RecentBillListVo>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        Ok(BillRepo::recent_bill(token, addr, chain_code, page, page_size, pool).await?)
    }

    pub async fn query_tx_result(req: Vec<String>) -> Result<Vec<BillEntity>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

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
    ) -> Result<BillEntity, crate::ServiceError> {
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

        // 对于服务费订单和部署多签账号订单，需要修改对应的多签账号的状态
        if sync_bill.tx_update.status == entities::bill::BillStatus::Success.to_i8() {
            Self::handle_tx_kind(&transaction).await?;
        }

        // query transaction and handle result
        let tx = pool.begin().await.map_err(|e| crate::SystemError::Service(e.to_string()))?;

        match Self::handle_pending_tx_status(&transaction, &sync_bill, tx).await? {
            Some(tx) => Ok(tx),
            None => Ok(transaction),
        }
    }

    async fn handle_pending_tx_status(
        transaction: &BillEntity,
        sync_bill: &SyncBillEntity,
        mut tx: sqlx::Transaction<'static, sqlx::Sqlite>,
    ) -> Result<Option<BillEntity>, crate::ServiceError> {
        let assets_id = AssetsId {
            chain_code: transaction.chain_code.clone(),
            symbol: transaction.symbol.clone(),
            address: transaction.owner.clone(),
            token_address: transaction.token.clone(),
        };

        // 2. 更新账单
        let tx_result = BillRepo::update(&sync_bill.tx_update, tx.as_mut()).await?;

        // 1. 更新余额
        AssetsEntity::update_balance(tx.as_mut(), &assets_id, &sync_bill.balance)
            .await
            .map_err(|e| crate::SystemError::Service(format!("update balance fail:{e:?}")))?;

        // 3. 如果queue_id 存在表示是多签交易，需要同步多签队列里面的状态
        if !transaction.queue_id.is_empty() {
            let status = if sync_bill.tx_update.status == BillStatus::Success.to_i8() {
                MultisigQueueStatus::Success
            } else {
                MultisigQueueStatus::Fail
            };
            let _ =
                MultisigQueueDaoV1::update_status(&transaction.queue_id, status, tx.as_mut()).await;
        }

        let _res = tx.commit().await;
        Ok(tx_result)
    }

    // 对不同kind的交易做不同类型的处理
    async fn handle_tx_kind(bill_detail: &BillEntity) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let tx_kind = BillKind::try_from(bill_detail.tx_kind).unwrap();
        match tx_kind {
            // deploy multisig account
            BillKind::DeployMultiSign => {
                let condition = vec![("deploy_hash", bill_detail.hash.as_str())];
                let account = MultisigAccountDaoV1::find_by_conditions(condition, &*pool)
                    .await
                    .map_err(|e| crate::ServiceError::Database(e.into()))?;

                if let Some(account) = account {
                    let status = MultisigAccountStatus::OnChain.to_i8();
                    MultisigAccountDaoV1::update_status(&account.id, Some(status), None, &*pool)
                        .await
                        .map_err(crate::ServiceError::Database)?;
                }
            }
            // transfer multisig service fee
            BillKind::ServiceCharge => {
                let condition = vec![("fee_hash", bill_detail.hash.as_str())];
                let account = MultisigAccountDaoV1::find_by_conditions(condition, &*pool)
                    .await
                    .map_err(|e| crate::ServiceError::Database(e.into()))?;

                if let Some(account) = account {
                    let status = MultisigAccountPayStatus::Paid.to_i8();
                    MultisigAccountDaoV1::update_status(&account.id, None, Some(status), &*pool)
                        .await
                        .map_err(crate::ServiceError::Database)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn get_tx_res(
        transaction: &BillEntity,
    ) -> Result<Option<SyncBillEntity>, crate::ServiceError> {
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

    pub async fn list_by_hashs(
        owner: String,
        hashs: Vec<String>,
    ) -> Result<Vec<BillEntity>, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        Ok(BillRepo::lists_by_hashs(&owner, hashs, &pool).await?)
    }
}
