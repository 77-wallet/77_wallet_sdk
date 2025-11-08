use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::{
        api_wallet::transfer::ApiTransferExReq,
        transaction::{self},
    },
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
    pub async fn api_transfer(&self, req: ApiTransferExReq) -> ReturnType<TransactionResult> {
        ApiTransService::new(self.ctx).transfer(req, BillKind::Transfer).await
    }

    pub async fn api_bill_detail(&self, tx_hash: &str, owner: &str) -> ReturnType<BillDetailVo> {
        ApiTransService::new(self.ctx).bill_detail(tx_hash, owner).await
    }

    pub async fn api_list_by_hashs(
        &self,
        owner: String,
        hashs: Vec<String>,
    ) -> ReturnType<Vec<BillEntity>> {
        ApiTransService::new(self.ctx).list_by_hashs(hashs, &owner).await
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
        ApiTransService::new(self.ctx)
            .bill_lists(
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
        token: &str,
        addr: &str,
        chain_code: &str,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<RecentBillListVo>> {
        ApiTransService::new(self.ctx).recent_bill(token, addr, chain_code, page, page_size).await
    }

    // // 单笔查询交易并处理
    pub async fn api_query_tx_result(&self, req: Vec<String>) -> ReturnType<Vec<BillEntity>> {
        ApiTransService::new(self.ctx).query_tx_result(req).await
    }
}

#[cfg(test)]
mod test {
    use crate::{
        request::{api_wallet::transfer::ApiTransferExReq, transaction::BaseTransferReq},
        test::env::get_manager,
    };

    use anyhow::Result;

    #[tokio::test]
    async fn test_api_transfer() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let from = "TQJgSU6DvFvpMC1ExSJ1UVsznPqcH5v8G4";
        let to = "TAiqQmkg3eGs429uTnXV14gxvJuZzVhowh";
        let value = "3";
        let chain_code = "tron";

        let symbol = "TRX";
        let req = ApiTransferExReq {
            base: BaseTransferReq::new(from, to, value, chain_code, symbol),
            password: "q1111111".to_string(),
            fee_setting: "".to_string(),
            signer: None,
        };
        let res = wallet_manager.api_transfer(req).await;
        tracing::info!("create sub wallet res: {res:?}");

        Ok(())
    }

    #[tokio::test]
    async fn test_api_recent_bill() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let token = "";
        let addr = "TQJgSU6DvFvpMC1ExSJ1UVsznPqcH5v8G4";
        let chain_code = "tron";
        let page = 0;

        let page_size = 10;
        let res = wallet_manager.api_recent_bill(token, addr, chain_code, page, page_size).await;
        tracing::info!("create sub wallet res: {res:?}");

        Ok(())
    }
}
