use crate::{
    api::ReturnType,
    manager::WalletManager,
    request::{
        api_wallet::transfer::ApiTransferExReq,
        transaction::{self},
    },
    response_vo::standard_wallet::transaction::{BillDetailVo, TransactionResult},
    service::{api_wallet::transaction::ApiTransService, transaction::TransactionService},
};
use serde_json;
use wallet_database::{
    entities::bill::{BillEntity, BillKind, RecentBillListVo},
    pagination::Pagination,
};

impl WalletManager {
    /// Estimates the transaction fee for a transfer request.
    pub async fn api_trans_fee(
        &self,
        req: transaction::BaseTransferReq,
    ) -> ReturnType<crate::response_vo::EstimateFeeResp> {
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
        tx_kind: Vec<i32>,
        transfer_type: Option<i32>,
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
                tx_kind,
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

    /// 统计归集订单执行耗时
    pub async fn api_collect_order_stats(&self) -> ReturnType<serde_json::Value> {
        use sqlx::Row;
        use wallet_database::CollectDbPool;

        // 获取数据库连接池
        let pool = self.ctx.api_funds_pool()?;

        // 1. 每笔订单的执行耗时
        let per_order_stats = sqlx::query(
            r#"
            SELECT 
                trade_no, 
                order_ack_sent_at    AS start_at, 
                finished_at          AS end_at, 
                CAST( 
                    (julianday(finished_at) - julianday(order_ack_sent_at)) * 86400 
                    AS INTEGER 
                ) AS cost_seconds 
            FROM api_collect 
            WHERE 
                order_ack_sent_at IS NOT NULL 
                AND finished_at IS NOT NULL 
            ORDER BY cost_seconds DESC
        "#,
        )
        .fetch_all(pool.as_ref())
        .await
        .unwrap();

        let mut per_order_stats_list = Vec::new();
        for row in per_order_stats {
            let trade_no: String = row.try_get("trade_no").unwrap();
            let start_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("start_at").unwrap();
            let end_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("end_at").unwrap();
            let cost_seconds: i64 = row.try_get("cost_seconds").unwrap();

            per_order_stats_list.push(serde_json::json!({
                "trade_no": trade_no,
                "start_at": start_at,
                "end_at": end_at,
                "cost_seconds": cost_seconds
            }));
        }

        // 2. 每一阶段耗时
        let stage_stats = sqlx::query(
            r#"
            SELECT 
                trade_no, 
                CAST((julianday(building_at) - julianday(order_ack_sent_at)) * 86400 AS INTEGER) 
                    AS build_cost_sec, 
                CAST((julianday(last_broadcast_at) - julianday(building_at)) * 86400 AS INTEGER) 
                    AS broadcast_cost_sec, 
                CAST((julianday(finished_at) - julianday(last_broadcast_at)) * 86400 AS INTEGER) 
                    AS chain_confirm_cost_sec 
            FROM api_collect 
            WHERE 
                order_ack_sent_at IS NOT NULL 
                AND building_at IS NOT NULL 
                AND last_broadcast_at IS NOT NULL 
                AND finished_at IS NOT NULL 
            ORDER BY chain_confirm_cost_sec DESC
        "#,
        )
        .fetch_all(pool.as_ref())
        .await
        .unwrap();

        let mut stage_stats_list = Vec::new();
        for row in stage_stats {
            let trade_no: String = row.try_get("trade_no").unwrap();
            let build_cost_sec: i64 = row.try_get("build_cost_sec").unwrap();
            let broadcast_cost_sec: i64 = row.try_get("broadcast_cost_sec").unwrap();
            let chain_confirm_cost_sec: i64 = row.try_get("chain_confirm_cost_sec").unwrap();

            stage_stats_list.push(serde_json::json!({
                "trade_no": trade_no,
                "build_cost_sec": build_cost_sec,
                "broadcast_cost_sec": broadcast_cost_sec,
                "chain_confirm_cost_sec": chain_confirm_cost_sec
            }));
        }

        // 3. 整批订单的总耗时
        let total_stats = sqlx::query(
            r#"
            SELECT 
                MIN(order_ack_sent_at) AS first_order_start, 
                MAX(finished_at)       AS last_order_end, 
                CAST( 
                    (julianday(MAX(finished_at)) - julianday(MIN(order_ack_sent_at))) * 86400 
                    AS INTEGER 
                ) AS total_cost_seconds 
            FROM api_collect 
            WHERE 
                order_ack_sent_at IS NOT NULL 
                AND finished_at IS NOT NULL
        "#,
        )
        .fetch_optional(pool.as_ref())
        .await
        .unwrap();

        let total_stats_json = if let Some(row) = total_stats {
            let first_order_start: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get("first_order_start").unwrap();
            let last_order_end: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get("last_order_end").unwrap();
            let total_cost_seconds: i64 = row.try_get("total_cost_seconds").unwrap();

            serde_json::json!({
                "first_order_start": first_order_start,
                "last_order_end": last_order_end,
                "total_cost_seconds": total_cost_seconds
            })
        } else {
            serde_json::json!({
                "first_order_start": null,
                "last_order_end": null,
                "total_cost_seconds": 0
            })
        };

        // 4. 组装结果
        let result = serde_json::json!({
            "per_order_stats": per_order_stats_list,
            "stage_stats": stage_stats_list,
            "total_stats": total_stats_json
        });

        Ok(result)
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod test {
    use crate::{request::api_wallet::transfer::ApiTransferExReq, test::env::get_manager};

    use crate::request::api_wallet::trans::ApiBaseTransferReq;
    use anyhow::Result;

    #[tokio::test]
    async fn test_api_transfer() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let from = "TQJgSU6DvFvpMC1ExSJ1UVsznPqcH5v8G4";
        let to = "TU5mG6jMohKBNiFR62t1pJG9fN6kJbVzF5";
        let value = "100";
        let chain_code = "tron";

        // let symbol = "TRX";
        let req = ApiTransferExReq {
            base: ApiBaseTransferReq::new(from, to, value, chain_code),
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

    #[tokio::test]
    async fn test_api_bill_lists() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        // wallet_manager.init_api_swap().await?;

        let page = 0;

        let page_size = 10;
        let res = wallet_manager
            .api_bill_lists(
                Some("0x7Ee2D3e497910faE4b8223Df2575C874CE8f3026".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                vec![],
                None,
                page,
                page_size,
            )
            .await?;
        let res = serde_json::to_string(&res)?;
        tracing::info!("create sub wallet res: {res}");

        Ok(())
    }

    #[tokio::test]
    async fn test_collect() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        let from = "TEMz9b6wMzJAc56JQJseWBKYqoMjYxXx91";

        let list = wallet_manager
            .list_api_wallet_account(
                "0x7F90ff4374cDFEF97c7Fd546B5E038E06a528166",
                None,
                Some("tron".to_string()),
                0,
                50,
            )
            .await?;
        let chain_code = "tron";
        let value = "3.7";
        let symbol = "TRX";

        for account in list.data {
            if let Some(chain) = account.chain.iter().find(|chain| chain.chain_code == chain_code) {
                let mut base = ApiBaseTransferReq::new(from, &chain.address, value, chain_code);
                base.with_token(None, 6, symbol);
                let req = ApiTransferExReq {
                    base,
                    password: "q1111111".to_string(),
                    fee_setting: "".to_string(),
                    signer: None,
                };
                let res = wallet_manager.api_transfer(req).await;
                tracing::info!("test_collect res: {res:?}");
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_api_transfer_to_subaccounts() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;
        let wallet_password = "q1111111";
        let _ = wallet_manager.set_passwd_cache(wallet_password).await;
        // let (tx, rx) =
        //     tokio::sync::mpsc::unbounded_channel::<crate::messaging::notify::FrontendNotifyEvent>();
        // let mut rx = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        // wallet_manager.set_frontend_notify_sender(tx).await?;

        // 定义根钱包地址
        let chain_code = "tron";
        let value = "5";
        let symbol = "TRX";

        // 获取第一个出款账户的地址
        let from_address = "TW6h166qfNfibxgovAnVyDDMNV1BFXp5A5";

        tracing::info!("Using from address: {}", from_address);

        let sub_wallet_addr = "0x5489c657Be2504D657f1F56AB04abfE3C77ceC34";
        // 获取子账户钱包的全部子账户
        let subaccounts = wallet_manager
            .list_api_wallet_account(sub_wallet_addr, None, Some(chain_code.to_string()), 0, 10)
            .await?;

        tracing::info!("Found {} subaccounts", subaccounts.data.len());

        // 让出款钱包的第一个账户循环给子账户转账
        for account in subaccounts.data {
            if let Some(chain) = account.chain.iter().find(|chain| chain.chain_code == chain_code) {
                tracing::info!(
                    "Transferring {} {} from {} to {}",
                    value,
                    symbol,
                    from_address,
                    chain.address
                );

                // 创建转账请求
                let mut base =
                    ApiBaseTransferReq::new(&from_address, &chain.address, value, chain_code);
                base.with_token(None, 6, symbol);
                let req = ApiTransferExReq {
                    base,
                    password: wallet_password.to_string(),
                    fee_setting: "".to_string(),
                    signer: None,
                };

                // 执行转账
                let res = wallet_manager.api_transfer(req).await;
                tracing::info!("Transfer to {} res: {res:?}", chain.address);
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_api_collect_order_stats() -> Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (wallet_manager, _test_params) = get_manager().await?;
        wallet_manager.init_api_swap().await?;

        // 调用统计方法
        let res = wallet_manager.api_collect_order_stats().await.unwrap();
        tracing::info!("Collect order stats res: {:?}", res);
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("Collect order stats res: {res}");
        Ok(())
    }
}
