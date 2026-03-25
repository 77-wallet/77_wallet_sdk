use crate::{
    context::Context,
    domain::{
        api_wallet::{
            adapter_factory::ApiChainAdapterFactory, coin::ApiCoinDomain, trans::ApiTransDomain,
        },
        app::config::ConfigDomain,
        bill::BillDomain,
        wallet::WalletDomain,
    },
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::nonce::nonce_engine::get_nonce_engine,
    request::api_wallet::{
        trans::{ApiBaseTransferReq, ApiTransferReq},
        transfer::ApiTransferExReq,
    },
    response_vo::standard_wallet::transaction::{BillDetailVo, TransactionResult},
};
use chrono::Utc;
use futures::future::join_all;
use std::collections::HashSet;
use wallet_chain_interact::{BillResourceConsume, types::ChainPrivateKey};
use wallet_database::{
    ApiTransactionDbPool, ApiWalletDbPool, CoreDbPool,
    entities::{
        api_trade_type::ApiTradeType,
        api_withdraw::{ApiWithdrawEntity, ApiWithdrawStatus},
        asset_token_key::AssetTokenKey,
        bill::{BillEntity, BillKind, RecentBillListVo, SyncBillEntity},
    },
    pagination::Pagination,
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, coin::ApiCoinRepo, nonce::ApiNonceRepo, wallet::ApiWalletRepo,
            withdraw::ApiWithdrawRepo,
        },
        bill::BillRepo,
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

    pub(crate) fn build_api_transfer_base(
        params: &ApiTransferExReq,
        token_key: &AssetTokenKey,
        decimals: u8,
    ) -> ApiBaseTransferReq {
        ApiBaseTransferReq {
            from: params.base.from.clone(),
            to: params.base.to.clone(),
            value: params.base.value.clone(),
            chain_code: params.base.chain_code.clone(),
            token_address: token_key.clone(),
            decimals,
            symbol: params.base.symbol.clone(),
            request_resource_id: params.base.request_resource_id.clone(),
            spend_all: params.base.spend_all,
            notes: params.base.notes.clone(),
            metadata: Some(params.fee_setting.clone()),
        }
    }

    async fn get_eth_nonce(&self, from_addr: &str, chain_code: &str) -> Result<i64, ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let nonce_engine = get_nonce_engine();
        let nonce = nonce_engine.allocate_nonce(from_addr, chain_code, &pool).await?;
        Ok(nonce as i64)
    }

    pub(crate) async fn get_transfer_nonce(
        &self,
        from_addr: &str,
        chain_code: &str,
        chain_code_enum: ChainCode,
    ) -> Result<i64, ServiceError> {
        match chain_code_enum {
            ChainCode::Ethereum | ChainCode::BnbSmartChain => {
                self.get_eth_nonce(from_addr, chain_code).await
            }
            _ => Ok(0),
        }
    }

    pub async fn transfer(
        &self,
        params: ApiTransferExReq,
        _bill_kind: BillKind,
    ) -> Result<TransactionResult, ServiceError> {
        WalletDomain::validate_password(&params.password).await?;

        let private_key = crate::domain::api_wallet::account::ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
        )
        .await?;
        self.transfer_with_private_key(params, private_key).await
    }

    pub(crate) async fn transfer_with_private_key(
        &self,
        params: ApiTransferExReq,
        private_key: ChainPrivateKey,
    ) -> Result<TransactionResult, ServiceError> {
        let from_addr = params.base.from.clone();
        let _gate = crate::infrastructure::nonce::nonce_engine::get_nonce_engine()
            .acquire_transfer_gate(&from_addr, &params.base.chain_code)
            .await;

        let pool = self.ctx.api_wallet_pool()?;
        let api_transaction_pool = self.ctx.api_transaction_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(
            &params.base.from,
            &params.base.chain_code,
            &pool,
        )
        .await?
        .ok_or(ServiceError::Business(ApiWalletError::NotFoundAccount.into()))?;
        let wallet = ApiWalletRepo::find_by_address(&pool, &account.wallet_address).await?.ok_or(
            ServiceError::Business(
                ApiWalletError::Wallet(
                    crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                )
                .into(),
            ),
        )?;

        let token_key = params.base.token_address.clone();
        let coin =
            ApiCoinDomain::get_coin_by_token_key_exact(&params.base.chain_code, token_key.clone())
                .await?;

        let chain_code = params.base.chain_code.as_str();
        let chain_code: ChainCode = chain_code.try_into()?;
        let nonce =
            self.get_transfer_nonce(&params.base.from, &params.base.chain_code, chain_code).await?;

        let req = ApiTransferReq {
            base: Self::build_api_transfer_base(&params, &token_key, coin.decimals),
            password: params.password.to_string(),
            nonce: nonce as u64,
        };
        let res = ApiTransDomain::transfer(req, Some(private_key)).await?;
        let resource_consume = res.resource_consume().unwrap_or_else(|_| "".to_string());
        let trade_no = uuid::Uuid::new_v4().to_string();
        ApiWithdrawRepo::upsert_api_withdraw(
            &api_transaction_pool,
            &wallet.uid,
            &wallet.name,
            &params.base.from,
            &params.base.to,
            &params.base.value,
            "",
            &params.base.chain_code,
            token_key,
            &params.base.symbol,
            &trade_no,
            ApiTradeType::SelfWithdraw,
            nonce,
            Some(res.tx_hash.clone()),
            ApiWithdrawStatus::Init,
            ApiWithdrawStatus::Init,
            resource_consume.as_str(),
            res.fee.as_str(),
            None,
            None,
        )
        .await?;
        ApiNonceRepo::set_nonce_floor(
            &api_transaction_pool,
            &params.base.from,
            &params.base.chain_code,
            nonce,
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

        let api_transaction_pool = self.ctx.api_transaction_pool()?;
        let core_pool = self.ctx.api_wallet_pool()?;
        let bill =
            ApiWithdrawRepo::get_by_hash_and_owner(&api_transaction_pool, owner, &tx_hash).await?;

        let main_coin = ApiCoinRepo::main_coin(&bill.chain_code, &core_pool).await?;
        let resource_consume = if !bill.resource_consume.is_empty() && bill.resource_consume != "0"
        {
            Some(BillResourceConsume::from_json_str(&bill.resource_consume)?)
        } else {
            None
        };
        let transfer_type = Self::default_transfer_type_by_trade_type(bill.trade_type);
        let e = self.convert_to_bill_entity(&bill, transfer_type);
        Ok(BillDetailVo {
            bill: e,
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
        let api_transaction_pool = self.ctx.api_transaction_pool()?;
        let bills = ApiWithdrawRepo::lists_by_hashs(&api_transaction_pool, owner, tx_hash).await?;

        let futures = bills.iter().map(|bill| async move {
            let transfer_type = Self::default_transfer_type_by_trade_type(bill.trade_type);
            let e = self.convert_to_bill_entity(&bill, transfer_type);
            Ok(e)
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
        tx_kind: Vec<i32>,
        transfer_type: Option<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<BillEntity>, ServiceError> {
        let pool = self.ctx.api_wallet_pool()?;
        let api_transaction_pool = self.ctx.api_transaction_pool()?;
        let uid = match root_addr.clone() {
            Some(addr) => {
                let wallet = ApiWalletRepo::find_by_address(&pool, addr.as_str()).await?.ok_or(
                    ServiceError::Business(
                        ApiWalletError::Wallet(
                            crate::error::business::api_wallet::wallet::WalletError::NotFound
                                .into(),
                        )
                        .into(),
                    ),
                )?;
                wallet
            }
            None => {
                return Err(ServiceError::Business(
                    ApiWalletError::Wallet(
                        crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                    )
                    .into(),
                ));
            }
        };
        let reference_addrs = self
            .build_reference_addrs(&pool, addr, root_addr.clone(), account_id, chain_code)
            .await?;
        let masked_reference_addrs =
            reference_addrs.iter().map(|item| Self::mask_addr(item)).collect::<Vec<_>>();
        tracing::info!(
            reference_addr_count = reference_addrs.len(),
            reference_addrs = ?masked_reference_addrs,
            account_id = ?account_id,
            "api_bill_lists resolved reference addresses"
        );

        // 过滤最小金额
        let min_value = match (symbol, filter_min_value) {
            (Some(symbol), Some(true)) => ConfigDomain::get_config_min_value(symbol).await?,
            _ => None,
        };
        let mut tx_kinds = vec![];
        for tx in tx_kind {
            if tx == BillKind::Transfer as i32 {
                if let Some(transfer) = transfer_type {
                    if transfer == 0 {
                        tx_kinds.push(ApiTradeType::SelfRecharge as i32);
                    } else if transfer == 1 {
                        tx_kinds.push(ApiTradeType::SelfWithdraw as i32);
                    }
                } else {
                    tx_kinds.push(ApiTradeType::SelfWithdraw as i32);
                    tx_kinds.push(ApiTradeType::SelfRecharge as i32);
                }
            } else if tx == BillKind::ApiWithdraw as i32 {
                tx_kinds.push(ApiTradeType::Withdraw as i32);
            }
        }

        let res = ApiWithdrawRepo::bill_lists(
            &api_transaction_pool,
            &uid.uid,
            &reference_addrs,
            chain_code,
            symbol,
            is_multisig,
            min_value,
            start,
            end,
            tx_kinds,
            page,
            page_size,
        )
        .await?;

        let selected_addrs = reference_addrs.into_iter().collect::<HashSet<String>>();
        let mut data = Vec::with_capacity(res.data.len());
        for item in &res.data {
            let from_hit = selected_addrs.contains(item.from_addr.as_str());
            let to_hit = selected_addrs.contains(item.to_addr.as_str());
            let transfer_type = Self::resolve_transfer_type(
                item.trade_type,
                &item.from_addr,
                &item.to_addr,
                &selected_addrs,
            );
            tracing::debug!(
                from_addr = %Self::mask_addr(&item.from_addr),
                to_addr = %Self::mask_addr(&item.to_addr),
                from_hit,
                to_hit,
                transfer_type,
                trade_type = item.trade_type as u8,
                "api_bill_lists transfer direction resolved"
            );
            data.push(self.convert_to_bill_entity(item, transfer_type));
        }

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
        let api_transaction_pool = crate::context::get_context()?.api_transaction_pool()?;
        let res = ApiWithdrawRepo::recent_bill(
            &api_transaction_pool,
            token,
            addr,
            chain_code,
            page,
            page_size,
        )
        .await;
        let mut data: Vec<RecentBillListVo> = vec![];
        let mut total_count = 0;
        match res {
            Ok(res) => {
                for it in res.data {
                    let transfer_type =
                        if it.trade_type == ApiTradeType::SelfRecharge { 0 } else { 1 };
                    let transaction_time = it.transaction_time.unwrap_or_else(Utc::now);
                    data.push(RecentBillListVo {
                        chain_code: it.chain_code,
                        symbol: it.symbol,
                        tx_hash: it.tx_hash.unwrap_or_default(),
                        value: it.value,
                        address: it.to_addr,
                        transaction_time,
                        transfer_type,
                        created_at: it.created_at,
                    })
                }
                total_count = res.total_count;
            }
            Err(_) => {}
        }

        let bill_res: Pagination<RecentBillListVo> =
            Pagination::<RecentBillListVo> { page, page_size, total_count, data };
        Ok(bill_res)
    }

    pub async fn query_tx_result(&self, req: Vec<String>) -> Result<Vec<BillEntity>, ServiceError> {
        let api_transaction_pool = crate::context::get_context()?.api_transaction_pool()?;
        let core_pool = crate::context::get_context()?.core_pool()?;
        let mut res = vec![];
        for id in req.iter() {
            match self.sync_bill_info(core_pool.clone(), &api_transaction_pool, id).await {
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
        core_pool: CoreDbPool,
        api_transaction_pool: &ApiTransactionDbPool,
        id: &str,
    ) -> Result<BillEntity, ServiceError> {
        let bill = ApiWithdrawRepo::get_api_withdraw_by_id(api_transaction_pool, id).await?;

        if bill.status != ApiWithdrawStatus::ConfirmSuccessReport
            || bill.status != ApiWithdrawStatus::ConfirmFailureReport
            || bill.status != ApiWithdrawStatus::SendingTxFailed
            || bill.status != ApiWithdrawStatus::AuditReject
        {
            let transfer_type = Self::default_transfer_type_by_trade_type(bill.trade_type);
            let e = self.convert_to_bill_entity(&bill, transfer_type);
            return Ok(e);
        }

        let sync_bill = match self.get_tx_res(&bill).await? {
            Some(tx_result) => tx_result,
            None => {
                // // 处理交易是否失败的逻辑
                // if bill.is_failed() {
                //     BillRepo::update_fail(&transaction.hash, &pool).await?;
                // }
                let transfer_type = Self::default_transfer_type_by_trade_type(bill.trade_type);
                let e = self.convert_to_bill_entity(&bill, transfer_type);
                return Ok(e);
            }
        };

        match self.handle_pending_tx_status(&sync_bill, core_pool).await? {
            Some(tx) => Ok(tx),
            None => {
                let transfer_type = Self::default_transfer_type_by_trade_type(bill.trade_type);
                let e = self.convert_to_bill_entity(&bill, transfer_type);
                Ok(e)
            }
        }
    }

    async fn handle_pending_tx_status(
        &self,
        sync_bill: &SyncBillEntity,
        pool: CoreDbPool,
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
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter(&transaction.chain_code).await?;

        let tx_hash = match transaction.tx_hash {
            Some(ref tx_hash) if !tx_hash.is_empty() => tx_hash,
            _ => return Ok(None),
        };

        let Some(tx_result) = adapter.query_tx_res(tx_hash).await? else {
            return Ok(None);
        };

        let token_key = transaction.token_addr.clone();

        // 查询余额
        let balance = adapter.balance_token_key(&transaction.from_addr, token_key.clone()).await?;

        let coin =
            ApiCoinDomain::get_coin_by_token_key_exact(&transaction.chain_code, token_key).await?;

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

    fn resolve_transfer_type(
        trade_type: ApiTradeType,
        from_addr: &str,
        to_addr: &str,
        selected_addrs: &HashSet<String>,
    ) -> i8 {
        let from_hit = selected_addrs.contains(from_addr);
        let to_hit = selected_addrs.contains(to_addr);
        if from_hit && !to_hit {
            1
        } else if to_hit && !from_hit {
            0
        } else {
            Self::default_transfer_type_by_trade_type(trade_type)
        }
    }

    fn default_transfer_type_by_trade_type(trade_type: ApiTradeType) -> i8 {
        if trade_type == ApiTradeType::SelfRecharge { 0 } else { 1 }
    }

    async fn build_reference_addrs(
        &self,
        pool: &ApiWalletDbPool,
        addr: Option<String>,
        root_addr: Option<String>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<Vec<String>, ServiceError> {
        if let Some(explicit_addr) = addr.filter(|item| !item.is_empty()) {
            return Ok(vec![explicit_addr]);
        }

        let chain_codes =
            if let Some(chain_code) = chain_code { vec![chain_code.to_string()] } else { vec![] };
        let accounts =
            ApiAccountRepo::api_account_list(pool, root_addr, account_id, chain_codes).await?;
        let fallback_addrs = accounts.into_iter().map(|item| item.address).collect::<Vec<_>>();
        Ok(Self::resolve_reference_addrs(None, fallback_addrs))
    }

    fn resolve_reference_addrs(addr: Option<String>, fallback_addrs: Vec<String>) -> Vec<String> {
        if let Some(explicit_addr) = addr.filter(|item| !item.is_empty()) {
            return vec![explicit_addr];
        }
        let mut seen = HashSet::new();
        fallback_addrs
            .into_iter()
            .filter(|item| !item.is_empty())
            .filter(|item| seen.insert(item.clone()))
            .collect()
    }

    fn mask_addr(addr: &str) -> String {
        if addr.len() <= 8 {
            addr.to_string()
        } else {
            format!("{}...{}", &addr[..4], &addr[addr.len() - 4..])
        }
    }

    fn convert_to_bill_entity(&self, bill: &ApiWithdrawEntity, transfer_type: i8) -> BillEntity {
        let tx_kind = if bill.trade_type == ApiTradeType::Withdraw {
            BillKind::ApiWithdraw
        } else {
            BillKind::Transfer
        };
        let transaction_time = bill.transaction_time.unwrap_or_else(Utc::now);
        let status = if bill.status == ApiWithdrawStatus::ConfirmSuccessReport {
            2
        } else if bill.status == ApiWithdrawStatus::ConfirmFailureReport
            || bill.status == ApiWithdrawStatus::SendingTxFailed
            || bill.status == ApiWithdrawStatus::AuditReject
        {
            3
        } else {
            1
        };
        BillEntity {
            id: bill.id as i32,
            hash: bill.tx_hash.clone().unwrap_or_default(),
            chain_code: bill.chain_code.to_string(),
            symbol: bill.symbol.to_string(),
            transfer_type,
            tx_kind: tx_kind as i8,
            owner: bill.from_addr.to_string(),
            from_addr: bill.from_addr.to_string(),
            to_addr: bill.to_addr.to_string(),
            token: bill.token_addr.clone(),
            value: bill.value.to_string(),
            resource_consume: bill.resource_consume.to_string(),
            transaction_fee: bill.transaction_fee.to_string(),
            transaction_time,
            status,
            is_multisig: 0,
            block_height: bill.block_height.clone().unwrap_or_default(),
            queue_id: "".to_string(),
            notes: bill.notes.clone().unwrap_or_default(),
            signer: bill.from_addr.to_string(),
            extra: "".to_string(),
            created_at: bill.created_at,
            updated_at: bill.updated_at,
        }
    }
}

#[cfg(test)]
mod transfer_token_tests {
    use super::ApiTransService;
    use crate::request::api_wallet::{trans::ApiBaseTransferReq, transfer::ApiTransferExReq};
    use wallet_database::entities::asset_token_key::AssetTokenKey;

    fn make_transfer_ex_req(token_address: Option<&str>) -> ApiTransferExReq {
        ApiTransferExReq {
            base: ApiBaseTransferReq {
                from: "from".to_string(),
                to: "to".to_string(),
                value: "1".to_string(),
                chain_code: "tron".to_string(),
                symbol: "TRX".to_string(),
                token_address: AssetTokenKey::from_raw(token_address),
                decimals: 0,
                request_resource_id: None,
                spend_all: false,
                notes: None,
                metadata: None,
            },
            password: "pwd".to_string(),
            fee_setting: "".to_string(),
            signer: None,
        }
    }

    #[test]
    fn build_api_transfer_base_native_token_uses_none_for_chain() {
        let params = make_transfer_ex_req(Some(""));
        let base = ApiTransService::build_api_transfer_base(&params, &AssetTokenKey::Native, 6);
        assert_eq!(base.token_address, AssetTokenKey::Native);
    }

    #[test]
    fn build_api_transfer_base_contract_token_preserved_for_chain() {
        let params = make_transfer_ex_req(Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"));
        let token = AssetTokenKey::from_raw(Some("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"));
        let base = ApiTransService::build_api_transfer_base(&params, &token, 6);
        assert_eq!(base.token_address, token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected_addrs(addrs: &[&str]) -> HashSet<String> {
        addrs.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn resolve_transfer_type_returns_incoming_when_to_addr_matches_selected_account() {
        let selected = selected_addrs(&["to-address"]);
        let transfer_type = ApiTransService::resolve_transfer_type(
            ApiTradeType::SelfWithdraw,
            "from-address",
            "to-address",
            &selected,
        );
        assert_eq!(transfer_type, 0);
    }

    #[test]
    fn resolve_transfer_type_returns_outgoing_when_from_addr_matches_selected_account() {
        let selected = selected_addrs(&["from-address"]);
        let transfer_type = ApiTransService::resolve_transfer_type(
            ApiTradeType::SelfRecharge,
            "from-address",
            "to-address",
            &selected,
        );
        assert_eq!(transfer_type, 1);
    }

    #[test]
    fn resolve_transfer_type_falls_back_to_trade_type_when_both_addrs_match() {
        let selected = selected_addrs(&["from-address", "to-address"]);
        let transfer_type = ApiTransService::resolve_transfer_type(
            ApiTradeType::SelfRecharge,
            "from-address",
            "to-address",
            &selected,
        );
        assert_eq!(transfer_type, 0);
    }

    #[test]
    fn resolve_transfer_type_falls_back_to_trade_type_when_neither_addr_matches() {
        let selected = selected_addrs(&["another-address"]);
        let transfer_type = ApiTransService::resolve_transfer_type(
            ApiTradeType::SelfWithdraw,
            "from-address",
            "to-address",
            &selected,
        );
        assert_eq!(transfer_type, 1);
    }

    #[test]
    fn resolve_reference_addrs_prefers_explicit_addr_over_fallback() {
        let resolved = ApiTransService::resolve_reference_addrs(
            Some("THLja2cJJxjbn4cUZZq6BRX8QHK1sxFbT4".to_string()),
            vec![
                "TW6h166qfNfibxgovAnVyDDMNV1BFXp5A5".to_string(),
                "TNG39Z4DZZj1Qgb5TLbHiUfZG1QtdmBcxv".to_string(),
            ],
        );
        assert_eq!(resolved, vec!["THLja2cJJxjbn4cUZZq6BRX8QHK1sxFbT4".to_string()]);
    }
}
