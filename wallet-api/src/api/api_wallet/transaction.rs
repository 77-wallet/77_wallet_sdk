use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::transaction::{self},
    response_vo::{
        self,
        transaction::{BillDetailVo, TransactionResult},
    },
    service::{api_wallet::transaction::ApiTransService, transaction::TransactionService},
};
use wallet_database::{
    entities::bill::{BillEntity, BillKind, RecentBillListVo},
    pagination::Pagination,
};

impl WalletManager {
    /// Estimates the transaction fee for a transfer request.
    pub async fn api_trans_fee(
        &self,
        req: transaction::BaseTransferReq,
    ) -> ReturnType<response_vo::EstimateFeeResp> {
        TransactionService::transaction_fee(req).await
    }

    /// tokenAddress前端必须传
    pub async fn api_transfer(
        &self,
        req: transaction::TransferReq,
    ) -> ReturnType<TransactionResult> {
        ApiTransService::transfer(req, BillKind::Transfer).await
    }

    pub async fn api_bill_detail(&self, tx_hash: &str, owner: &str) -> ReturnType<BillDetailVo> {
        ApiTransService::bill_detail(tx_hash, owner).await
    }

    pub async fn api_list_by_hashs(
        &self,
        owner: String,
        hashs: Vec<String>,
    ) -> ReturnType<Vec<BillEntity>> {
        TransactionService::list_by_hashs(owner, hashs).await
    }

    pub async fn api_bill_lists(
        &self,
        root_addr: Option<String>,
        account_id: Option<u32>,
        is_multisig: Option<i64>,
        addr: Option<String>,
        chain_code: Option<String>,
        symbol: Option<String>,
        filter_min_value: Option<bool>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<BillEntity>> {
        ApiTransService::bill_lists(
            root_addr,
            account_id,
            addr,
            chain_code.as_deref(),
            symbol.as_deref(),
            is_multisig,
            filter_min_value,
            start,
            end,
            transfer_type,
            page,
            page_size,
        )
        .await
    }

    // 最近交易列表
    pub async fn api_recent_bill(
        &self,
        token: String,
        addr: String,
        chain_code: String,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<RecentBillListVo>> {
        TransactionService::recent_bill(&token, &addr, &chain_code, page, page_size).await
    }

    // // 单笔查询交易并处理
    pub async fn api_query_tx_result(&self, req: Vec<String>) -> ReturnType<Vec<BillEntity>> {
        ApiTransService::query_tx_result(req).await
    }
}
