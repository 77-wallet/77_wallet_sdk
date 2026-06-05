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
        standard_wallet::{
            account::Balance,
            transaction::{BillDetailVo, TransactionResult},
        },
    },
};
use wallet_database::{
    CoreDbPool, entities,
    entities::{
        asset_token_key::AssetTokenKey,
        assets::AssetsId,
        bill::{BillEntity, BillKind, BillStatus, RecentBillListVo, SyncBillEntity},
        multisig_account::{MultisigAccountPayStatus, MultisigAccountStatus},
        multisig_queue::{MemberSignedResult, MultisigQueueStatus},
    },
    pagination::Pagination,
    repositories::{
        account::AccountRepo, address_book::AddressBookRepo, assets::AssetsRepo, bill::BillRepo,
        coin::CoinRepo, multisig_account::MultisigAccountRepo, multisig_queue::MultisigQueueRepo,
    },
};
use wallet_utils::unit;

pub struct TransactionService {
    ctx: &'static crate::context::Context,
}

impl TransactionService {
    pub fn new(ctx: &'static crate::context::Context) -> Self {
        Self { ctx }
    }

    // 本币的余额
    pub async fn chain_balance(
        &self,
        address: &str,
        chain_code: &str,
        symbol: &str,
        token_key: AssetTokenKey,
    ) -> Result<Balance, crate::error::service::ServiceError> {
        tracing::info!(
            address = %address,
            chain_code = %chain_code,
            request_symbol = %symbol,
            request_token_address = %token_key.as_db_str(),
            "chain_balance request start"
        );
        let adapter =
            ChainAdapterFactory::get_transaction_adapter_with_ctx(&self.ctx, chain_code).await?;

        let coin = match CoinDomain::get_coin_by_token_key_with_ctx(
            self.ctx,
            chain_code,
            token_key.clone(),
        )
        .await
        {
            Ok(coin) => coin,
            Err(error) => {
                tracing::warn!(
                    chain_code = %chain_code,
                    request_symbol = %symbol,
                    request_token_address = %token_key.as_db_str(),
                    error = %error,
                    "chain_balance failed to resolve coin metadata"
                );
                return Err(error);
            }
        };
        tracing::info!(
            address = %address,
            chain_code = %chain_code,
            request_symbol = %symbol,
            resolved_symbol = %coin.symbol,
            request_token_address = %token_key.as_db_str(),
            resolved_token_address = ?coin.token_address.as_db_str(),
            "chain_balance resolved coin metadata"
        );
        let resolved_token_key = coin.token_address.clone();
        let resolved_token_address = resolved_token_key.to_option_string_for_api();
        let balance = match adapter.balance(address, resolved_token_key).await {
            Ok(balance) => balance,
            Err(error) => {
                tracing::warn!(
                    address = %address,
                    chain_code = %chain_code,
                    request_symbol = %symbol,
                    resolved_symbol = %coin.symbol,
                    request_token_address = %token_key.as_db_str(),
                    resolved_token_address = ?resolved_token_address,
                    error = %error,
                    "chain_balance failed to fetch on-chain balance"
                );
                return Err(error.into());
            }
        };
        tracing::info!(
            address = %address,
            chain_code = %chain_code,
            request_symbol = %symbol,
            resolved_symbol = %coin.symbol,
            request_token_address = %token_key.as_db_str(),
            resolved_token_address = ?resolved_token_address,
            on_chain_balance = %balance,
            "chain_balance fetched on-chain balance"
        );
        let format_balance = unit::format_to_string(balance, coin.decimals)?;
        tracing::info!(
            address = %address,
            chain_code = %chain_code,
            request_symbol = %symbol,
            resolved_symbol = %coin.symbol,
            request_token_address = %token_key.as_db_str(),
            resolved_token_address = ?resolved_token_address,
            formatted_balance = %format_balance,
            "chain_balance formatted balance"
        );
        let balance = Balance {
            balance: format_balance.clone(),
            decimals: coin.decimals,
            original_balance: balance.to_string(),
        };

        ChainTransDomain::update_balance_with_ctx(
            self.ctx,
            address,
            chain_code,
            &coin.symbol,
            coin.token_address.clone(),
            &format_balance,
        )
        .await?;

        tracing::info!(
            address = %address,
            chain_code = %chain_code,
            request_symbol = %symbol,
            resolved_symbol = %coin.symbol,
            request_token_address = %token_key.as_db_str(),
            resolved_token_address = ?resolved_token_address,
            coin_decimals = coin.decimals,
            "chain_balance request success"
        );

        Ok(balance)
    }

    /// 计算交易的手续费
    pub async fn transaction_fee(
        &self,
        mut params: transaction::BaseTransferReq,
    ) -> Result<response_vo::EstimateFeeResp, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;

        let token_key = params.token_address.clone();
        let coin = CoinRepo::coin_by_chain_token_key(&params.chain_code, token_key, &pool).await?;

        params.with_decimals(coin.decimals);
        params.with_token(coin.token_address.clone());

        let main_coin = CoinRepo::main_coin(&params.chain_code, &pool).await?;

        let adapter =
            ChainAdapterFactory::get_transaction_adapter_with_ctx(&self.ctx, &params.chain_code)
                .await?;
        let backend_api = self.ctx.get_global_backend_api();
        let fee = adapter
            .estimate_fee_with_ctx(params, main_coin.symbol.as_str(), backend_api.as_ref())
            .await?;

        let fee_resp =
            response_vo::EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), fee);
        Ok(fee_resp)
    }

    pub async fn transfer(
        &self,
        params: transaction::TransferReq,
        bill_kind: BillKind,
    ) -> Result<TransactionResult, crate::error::service::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter_with_ctx(
            &self.ctx,
            &params.base.chain_code,
        )
        .await?;

        let private_key = ChainTransDomain::get_key_with_ctx(
            self.ctx,
            &params.base.from,
            &params.base.chain_code,
            &params.password,
            &params.signer,
        )
        .await?;

        let tx_hash =
            ChainTransDomain::transfer_with_ctx(self.ctx, params, bill_kind, &adapter, private_key)
                .await?;
        Ok(TransactionResult { tx_hash })
    }

    async fn handle_queue_member(
        bill: &BillEntity,
        pool: CoreDbPool,
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
        let queue = match domain::multisig::MultisigDomain::queue_by_id(
            &bill.queue_id,
            &pool.clone().into_inner(),
        )
        .await
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
        &self,
        tx_hash: &str,
        owner: &str,
    ) -> Result<BillDetailVo, crate::error::service::ServiceError> {
        let tx_hash = BillDomain::handle_hash(tx_hash);

        let pool = self.ctx.core_pool()?;
        let core_pool = self.ctx.core_pool()?;

        let mut bill = BillRepo::get_by_hash_and_owner(&tx_hash, owner, &pool).await?;
        bill.truncate_to_8_decimals();

        let signature = Self::handle_queue_member(&bill, pool.clone()).await;

        let main_coin = CoinRepo::main_coin(&bill.chain_code, &pool).await?;

        let mut res = BillDetailVo::new(bill, main_coin.symbol, signature)?;

        // 根据地址和链获取钱包名称
        if !res.bill.to_addr.is_empty() {
            let account = AccountRepo::account_with_wallet(
                &res.bill.to_addr,
                &res.bill.chain_code,
                core_pool.clone(),
            )
            .await;
            if let Ok(account) = account {
                res.wallet_name = account.wallet_name;
                res.account_name = account.name;
            }
        }

        Ok(res)
    }

    pub async fn recent_bill(
        &self,
        token: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<RecentBillListVo>, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;

        Ok(BillRepo::recent_bill(token, addr, chain_code, page, page_size, pool).await?)
    }

    pub async fn query_tx_result(
        &self,
        req: Vec<String>,
    ) -> Result<Vec<BillEntity>, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;

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
        pool: CoreDbPool,
    ) -> Result<BillEntity, crate::error::service::ServiceError> {
        let transaction = BillRepo::find_by_id(id, &pool).await?;

        if transaction.status != wallet_database::entities::bill::BillStatus::Pending.to_i8() {
            return Ok(transaction);
        }

        // 不处理swap 类型的交易
        if transaction.tx_kind == BillKind::Swap.to_i8() {
            return Ok(transaction);
        }

        let sync_bill = match self.get_tx_res(&transaction).await? {
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
            Self::handle_tx_kind(&transaction, &pool).await?;
        }

        // query transaction and handle result
        let tx = pool.as_ref().begin().await.map_err(|e| {
            crate::error::service::ServiceError::System(crate::error::system::SystemError::Service(
                e.to_string(),
            ))
        })?;

        match Self::handle_pending_tx_status(&transaction, &sync_bill, tx).await? {
            Some(tx) => Ok(tx),
            None => Ok(transaction),
        }
    }

    async fn handle_pending_tx_status(
        transaction: &BillEntity,
        sync_bill: &SyncBillEntity,
        mut tx: sqlx::Transaction<'static, sqlx::Sqlite>,
    ) -> Result<Option<BillEntity>, crate::error::service::ServiceError> {
        let assets_id = AssetsId {
            chain_code: transaction.chain_code.clone(),
            address: transaction.owner.clone(),
            token_address: transaction.token.clone().into(),
        };

        // 2. 更新账单
        let tx_result = BillRepo::update(&sync_bill.tx_update, tx.as_mut()).await?;

        // 1. 更新余额
        AssetsRepo::update_balance_with_executor(&mut tx, &assets_id, &sync_bill.balance)
            .await
            .map_err(|e| {
                crate::error::service::ServiceError::System(
                    crate::error::system::SystemError::Service(e.to_string()),
                )
            })?;

        // 3. 如果queue_id 存在表示是多签交易，需要同步多签队列里面的状态
        if !transaction.queue_id.is_empty() {
            let status = if sync_bill.tx_update.status == BillStatus::Success.to_i8() {
                MultisigQueueStatus::Success
            } else {
                MultisigQueueStatus::Fail
            };
            let _ = MultisigQueueRepo::update_status_with_executor(
                &transaction.queue_id,
                status,
                tx.as_mut(),
            )
            .await;
        }

        let _res = tx.commit().await;
        Ok(tx_result)
    }

    // 对不同kind的交易做不同类型的处理
    async fn handle_tx_kind(
        bill_detail: &BillEntity,
        pool: &CoreDbPool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let tx_kind = BillKind::try_from(bill_detail.tx_kind).unwrap();
        match tx_kind {
            // deploy multisig account
            BillKind::DeployMultiSign => {
                let account =
                    MultisigAccountRepo::find_by_condition(&pool, "deploy_hash", &bill_detail.hash)
                        .await?;

                if let Some(account) = account {
                    let status = MultisigAccountStatus::OnChain.to_i8();
                    MultisigAccountRepo::update_status(&pool, &account.id, Some(status), None)
                        .await?;
                }
            }
            // transfer multisig service fee
            BillKind::ServiceCharge => {
                let account =
                    MultisigAccountRepo::find_by_condition(&pool, "fee_hash", &bill_detail.hash)
                        .await?;

                if let Some(account) = account {
                    let status = MultisigAccountPayStatus::Paid.to_i8();
                    MultisigAccountRepo::update_status(&pool, &account.id, None, Some(status))
                        .await?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn get_tx_res(
        &self,
        transaction: &BillEntity,
    ) -> Result<Option<SyncBillEntity>, crate::error::service::ServiceError> {
        let adapter =
            domain::chain::adapter::ChainAdapterFactory::get_transaction_adapter_with_ctx(
                &self.ctx,
                &transaction.chain_code,
            )
            .await?;

        let Some(tx_result) = adapter.query_tx_res(&transaction.hash).await? else {
            return Ok(None);
        };

        let token_key = transaction.token.clone();

        // 查询余额
        let balance = adapter.balance(&transaction.owner, token_key.clone()).await?;

        let coin = CoinDomain::get_coin_by_token_key_with_ctx(
            self.ctx,
            &transaction.chain_code,
            token_key,
        )
        .await?;

        let balance = unit::format_to_string(balance, coin.decimals)?;

        let tx_bill = BillRepo::build_bill_update(
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
        &self,
        owner: String,
        hashs: Vec<String>,
    ) -> Result<Vec<BillEntity>, crate::error::service::ServiceError> {
        let pool = self.ctx.core_pool()?;

        Ok(BillRepo::lists_by_hashs(&owner, hashs, &pool).await?)
    }
}
