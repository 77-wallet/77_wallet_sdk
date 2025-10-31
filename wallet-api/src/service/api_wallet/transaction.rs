use crate::{
    context::Context,
    domain::{
        self,
        api_wallet::{adapter_factory::ApiChainAdapterFactory, trans::ApiTransDomain},
        app::config::ConfigDomain,
        bill::BillDomain,
        coin::CoinDomain,
    },
    error::{
        business::{api_wallet::ApiWalletError, wallet::WalletError},
        service::ServiceError,
    },
    request::{
        api_wallet::{
            trans::{ApiBaseTransferReq, ApiTransferReq},
            transfer::ApiTransferExReq,
        },
        transaction::{self},
    },
    response_vo::transaction::{BillDetailVo, TransactionResult},
};
use alloy::primitives::TxKind;
use futures::future::join_all;
use wallet_chain_interact::BillResourceConsume;
use wallet_database::{
    entities::{
        api_trade_type::ApiWithdrawTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
        bill::{BillEntity, BillKind, BillUpdateEntity, RecentBillListVo, SyncBillEntity},
    },
    pagination::Pagination,
    repositories::{
        api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo, withdraw::ApiWithdrawRepo},
        bill::BillRepo,
        coin::CoinRepo,
    },
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::unit;

pub struct ApiTransService {
    ctx: &'static Context,
}

impl ApiTransService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn transfer(
        &self,
        params: ApiTransferExReq,
        bill_kind: BillKind,
    ) -> Result<TransactionResult, ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        let params1 = params.clone();
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params1.base.from,
            &params1.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(ServiceError::Business(ApiWalletError::NotFoundAccount.into()))?;
        let wallet = ApiWalletRepo::find_by_address(&pool, &account.wallet_address)
            .await?
            .ok_or(ServiceError::Business(ApiWalletError::WalletDoesNotExist.into()))?;

        let req = ApiTransferReq {
            base: ApiBaseTransferReq {
                from: params.base.from,
                to: params.base.to,
                value: params.base.value,
                chain_code: params.base.chain_code,
                token_address: params.base.token_address,
                decimals: params.base.decimals,
                symbol: params.base.symbol,
                request_resource_id: params.base.request_resource_id,
                spend_all: params.base.spend_all,
                notes: params.base.notes,
            },
            password: params.password.to_string(),
        };
        let res = ApiTransDomain::transfer(req).await?;

        let trade_no = uuid::Uuid::new_v4().to_string();
        ApiWithdrawRepo::upsert_api_withdraw(
            &pool,
            &wallet.uid,
            &wallet.name,
            &params1.base.from,
            &params1.base.to,
            &params1.base.value,
            "",
            &params1.base.chain_code,
            params1.base.token_address.clone(),
            &params1.base.symbol,
            &trade_no,
            ApiWithdrawTradeType::SelfWithdraw,
            &res.tx_hash,
            ApiWithdrawStatus::Init,
            "",
            None,
            "",
        )
        .await?;

        Ok(TransactionResult { tx_hash: res.tx_hash })
    }

    pub async fn bill_detail(
        &self,
        tx_hash: &str,
        owner: &str,
    ) -> Result<BillDetailVo, ServiceError> {
        let tx_hash = BillDomain::handle_hash(tx_hash);

        let pool = self.ctx.get_global_sqlite_pool()?;
        let bill = ApiWithdrawRepo::get_by_hash_and_owner(&pool, owner, &tx_hash).await?;

        let main_coin = CoinRepo::main_coin(&bill.chain_code, &pool).await?;

        let resource_consume = if !bill.resource_consume.is_empty() && bill.resource_consume != "0"
        {
            Some(BillResourceConsume::from_json_str(&bill.resource_consume)?)
        } else {
            None
        };

        let transfer_type =
            if bill.trade_type == ApiWithdrawTradeType::SelfRecharge { 0 } else { 1 };
        let tx_kind = if bill.trade_type == ApiWithdrawTradeType::Withdraw {
            BillKind::ApiWithdraw
        } else {
            BillKind::Transfer
        };
        Ok(BillDetailVo {
            bill: BillEntity {
                id: bill.id as i32,
                hash: bill.tx_hash.to_string(),
                chain_code: bill.chain_code.to_string(),
                symbol: bill.symbol.to_string(),
                transfer_type: transfer_type,
                tx_kind: tx_kind as i8,
                owner: bill.from_addr.to_string(),
                from_addr: bill.from_addr.to_string(),
                to_addr: bill.to_addr.to_string(),
                token: bill.token_addr.clone(),
                value: bill.value.to_string(),
                resource_consume: bill.resource_consume.to_string(),
                transaction_fee: bill.transaction_fee.to_string(),
                transaction_time: bill.transaction_time.unwrap(),
                status: 0,
                is_multisig: 0,
                block_height: bill.block_height,
                queue_id: "".to_string(),
                notes: bill.notes.clone(),
                signer: "".to_string(),
                extra: "".to_string(),
                created_at: bill.created_at,
                updated_at: bill.updated_at,
            },
            resource_consume,
            fee_symbol: main_coin.symbol.to_string(),
            signature: None,
            wallet_name: "".to_string(),
            account_name: "".to_string(),
        })
    }

    pub async fn list_by_hashs(
        &self,
        tx_hash: Vec<String>,
        owner: &str,
    ) -> Result<Vec<BillEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut bills = ApiWithdrawRepo::lists_by_hashs(&pool, owner, tx_hash).await?;

        let futures = bills.iter().map(|bill| async move {
            Ok(BillEntity {
                id: bill.id as i32,
                hash: bill.tx_hash.to_string(),
                chain_code: bill.chain_code.to_string(),
                symbol: bill.symbol.to_string(),
                transfer_type: 0,
                tx_kind: BillKind::Transfer as i8,
                owner: bill.from_addr.to_string(),
                from_addr: bill.from_addr.to_string(),
                to_addr: bill.to_addr.to_string(),
                token: bill.token_addr.clone(),
                value: bill.value.to_string(),
                resource_consume: bill.resource_consume.to_string(),
                transaction_fee: bill.transaction_fee.to_string(),
                transaction_time: bill.transaction_time.unwrap(),
                status: 0,
                is_multisig: 0,
                block_height: bill.block_height.to_string(),
                queue_id: "".to_string(),
                notes: bill.notes.clone(),
                signer: "".to_string(),
                extra: "".to_string(),
                created_at: bill.created_at,
                updated_at: bill.updated_at,
            })
        });
        let results: Vec<Result<BillEntity, ServiceError>> = join_all(futures).await;
        let results: Result<Vec<BillEntity>, ServiceError> = results.into_iter().collect();

        Ok(results?)
    }

    pub async fn bill_lists(
        &self,
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
        let uid = match root_addr.clone() {
            Some(addr) => {
                let wallet = ApiWalletRepo::find_by_address(&pool, addr.as_str())
                    .await?
                    .ok_or(ServiceError::Business(ApiWalletError::WalletDoesNotExist.into()))?;
                wallet
            }
            None => {
                return Err(ServiceError::Business(ApiWalletError::WalletDoesNotExist.into()));
            }
        };
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

        let mut res = ApiWithdrawRepo::bill_lists(
            &pool,
            &uid.uid,
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
        .await?;

        let data = res
            .data
            .iter_mut()
            .map(|item| {
                let transfer_type =
                    if item.trade_type == ApiWithdrawTradeType::SelfRecharge { 0 } else { 1 };
                let tx_kind = if item.trade_type == ApiWithdrawTradeType::Withdraw {
                    BillKind::ApiWithdraw
                } else {
                    BillKind::Transfer
                };
                let status = if item.status == ApiWithdrawStatus::ConfirmSuccessReport {
                    2
                } else if item.status == ApiWithdrawStatus::ConfirmFailureReport
                    || item.status == ApiWithdrawStatus::SendingTxFailed
                    || item.status == ApiWithdrawStatus::AuditReject
                {
                    3
                } else {
                    1
                };
                BillEntity {
                    id: item.id as i32,
                    hash: item.tx_hash.to_string(),
                    chain_code: item.chain_code.to_string(),
                    symbol: item.symbol.to_string(),
                    transfer_type: transfer_type,
                    tx_kind: tx_kind as i8,
                    owner: item.from_addr.to_string(),
                    from_addr: item.from_addr.to_string(),
                    to_addr: item.to_addr.to_string(),
                    token: item.token_addr.clone(),
                    value: item.value.to_string(),
                    resource_consume: item.resource_consume.to_string(),
                    transaction_fee: item.transaction_fee.to_string(),
                    transaction_time: item.transaction_time.unwrap(),
                    status: status,
                    is_multisig: 0,
                    block_height: item.block_height.to_string(),
                    queue_id: "".to_string(),
                    notes: item.notes.to_string(),
                    signer: "".to_string(),
                    extra: "".to_string(),
                    created_at: Default::default(),
                    updated_at: None,
                }
            })
            .collect::<Vec<BillEntity>>();

        let bill_res: Pagination<BillEntity> = Pagination::<BillEntity> {
            page: res.page,
            page_size: res.page_size,
            total_count: res.total_count,
            data,
        };
        Ok(bill_res)
    }

    pub async fn recent_bill(
        &self,
        token: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<RecentBillListVo>, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res =
            ApiWithdrawRepo::recent_bill(&pool, token, addr, chain_code, page, page_size).await?;
        let mut data: Vec<RecentBillListVo> = vec![];
        for it in res.data {
            data.push(RecentBillListVo {
                chain_code: it.chain_code.to_string(),
                symbol: it.symbol.to_string(),
                tx_hash: it.tx_hash.to_string(),
                value: it.value.to_string(),
                address: it.from_addr.to_string(),
                transaction_time: it.transaction_time.unwrap(),
                transfer_type: BillKind::Transfer as i8,
                created_at: it.created_at,
            })
        }
        let bill_res: Pagination<RecentBillListVo> = Pagination::<RecentBillListVo> {
            page: res.page,
            page_size: res.page_size,
            total_count: res.total_count,
            data,
        };
        Ok(bill_res)
    }

    pub async fn query_tx_result(&self, req: Vec<String>) -> Result<Vec<BillEntity>, ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let mut res = vec![];
        for id in req.iter() {
            match self.sync_bill_info(id, pool.clone()).await {
                Ok(tx) => res.push(tx),
                Err(e) => {
                    tracing::warn!("sync bill err id = {},err = {}", id, e)
                }
            }
        }
        Ok(res)
    }

    async fn sync_bill_info(
        &self,
        id: &str,
        pool: wallet_database::DbPool,
    ) -> Result<BillEntity, ServiceError> {
        let bill = ApiWithdrawRepo::get_by_hash_and_owner(&pool, "", id).await?;

        if bill.status != ApiWithdrawStatus::ConfirmSuccessReport
            || bill.status != ApiWithdrawStatus::ConfirmFailureReport
        {
            return Ok(BillEntity {
                id: bill.id as i32,
                hash: bill.tx_hash.to_string(),
                chain_code: bill.chain_code.to_string(),
                symbol: bill.symbol.to_string(),
                transfer_type: 0,
                tx_kind: BillKind::Transfer as i8,
                owner: bill.from_addr.to_string(),
                from_addr: bill.from_addr.to_string(),
                to_addr: bill.to_addr.to_string(),
                token: bill.token_addr.clone(),
                value: bill.value.to_string(),
                resource_consume: bill.resource_consume.to_string(),
                transaction_fee: bill.transaction_fee.to_string(),
                transaction_time: bill.transaction_time.unwrap(),
                status: 0,
                is_multisig: 0,
                block_height: bill.block_height.to_string(),
                queue_id: "".to_string(),
                notes: bill.notes.clone(),
                signer: "".to_string(),
                extra: "".to_string(),
                created_at: bill.created_at,
                updated_at: bill.updated_at,
            });
        }

        let sync_bill = match self.get_tx_res(&bill).await? {
            Some(tx_result) => tx_result,
            None => {
                // // 处理交易是否失败的逻辑
                // if bill.is_failed() {
                //     BillRepo::update_fail(&transaction.hash, &pool).await?;
                // }
                return Ok(BillEntity {
                    id: bill.id as i32,
                    hash: bill.tx_hash.to_string(),
                    chain_code: bill.chain_code.to_string(),
                    symbol: bill.symbol.to_string(),
                    transfer_type: 0,
                    tx_kind: BillKind::Transfer as i8,
                    owner: bill.from_addr.to_string(),
                    from_addr: bill.from_addr.to_string(),
                    to_addr: bill.to_addr.to_string(),
                    token: bill.token_addr.clone(),
                    value: bill.value.to_string(),
                    resource_consume: bill.resource_consume.to_string(),
                    transaction_fee: bill.transaction_fee.to_string(),
                    transaction_time: bill.transaction_time.unwrap(),
                    status: 0,
                    is_multisig: 0,
                    block_height: bill.block_height.to_string(),
                    queue_id: "".to_string(),
                    notes: bill.notes.clone(),
                    signer: "".to_string(),
                    extra: "".to_string(),
                    created_at: bill.created_at,
                    updated_at: bill.updated_at,
                });
            }
        };

        match self.handle_pending_tx_status(&bill, &sync_bill, &pool).await? {
            Some(tx) => Ok(tx),
            None => Ok(BillEntity {
                id: bill.id as i32,
                hash: bill.tx_hash.to_string(),
                chain_code: bill.chain_code.to_string(),
                symbol: bill.symbol.to_string(),
                transfer_type: 0,
                tx_kind: BillKind::Transfer as i8,
                owner: bill.from_addr.to_string(),
                from_addr: bill.from_addr.to_string(),
                to_addr: bill.to_addr.to_string(),
                token: bill.token_addr.clone(),
                value: bill.value.to_string(),
                resource_consume: bill.resource_consume.to_string(),
                transaction_fee: bill.transaction_fee.to_string(),
                transaction_time: bill.transaction_time.unwrap(),
                status: 0,
                is_multisig: 0,
                block_height: bill.block_height.to_string(),
                queue_id: "".to_string(),
                notes: bill.notes.clone(),
                signer: "".to_string(),
                extra: "".to_string(),
                created_at: bill.created_at,
                updated_at: bill.updated_at,
            }),
        }
    }

    async fn handle_pending_tx_status(
        &self,
        transaction: &ApiWithdrawEntity,
        sync_bill: &SyncBillEntity,
        pool: &wallet_database::DbPool,
    ) -> Result<Option<BillEntity>, ServiceError> {
        // 1. 更新账单
        let tx_result = BillRepo::update(&sync_bill.tx_update, pool.as_ref()).await?;

        // 2. 更新余额
        // ApiAssetsRepo::update_balance(
        //     pool,
        //     &transaction.owner,
        //     &transaction.chain_code,
        //     transaction.token.clone(),
        //     &sync_bill.balance,
        // )
        // .await?;

        Ok(tx_result)
    }

    async fn get_tx_res(
        &self,
        transaction: &ApiWithdrawEntity,
    ) -> Result<Option<SyncBillEntity>, ServiceError> {
        let chain_code: ChainCode = transaction.chain_code.as_str().try_into()?;
        let adapter = ApiChainAdapterFactory::new_transaction_adapter(chain_code).await?;

        let Some(tx_result) = adapter.query_tx_res(&transaction.tx_hash).await? else {
            return Ok(None);
        };

        let token = transaction
            .token_addr
            .as_ref()
            .filter(|token| !token.is_empty())
            .map(|token| token.to_string());

        // 查询余额
        let balance = adapter.balance(&transaction.from_addr, token.clone()).await?;

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
