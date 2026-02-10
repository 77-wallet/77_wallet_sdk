use crate::messaging::notify::{FrontendNotifyEvent, event::NotifyEvent};
use wallet_database::repositories::api_wallet::{collect::ApiCollectRepo, wallet::ApiWalletRepo};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = AWM_CMD_FEE_RES
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdFeeResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    trade_type: u32,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    uid: String,
}

// API手续费结果事件
impl AwmCmdFeeResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        self.check_uid().await?;

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;

        let data = NotifyEvent::AwmCmdFeeRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        let ctx = crate::context::CONTEXT.get().unwrap();
        let pool = ctx.api_wallet_pool()?;
        let res = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;
        if res.is_none() {
            return Ok(());
        }

        // FeeRes 表示外部“手续费问题已解决/手续费已到”的事实
        // Collect 流程的推进依赖 need_service_fee 从 true 变为 false
        if !self.status {
            return Ok(());
        }

        // 尝试对 api_collect 写入“解除构建阻断”事实
        // - 仅在 need_service_fee = true 时生效（幂等保护）
        // - 若 trade_no 不存在或 need_service_fee 非 true，则 rows_affected = 0
        let funds_pool = ctx.api_funds_pool()?;
        match ApiCollectRepo::get_api_collect_by_trade_no(&funds_pool, &self.trade_no).await {
            Ok(collect) => {
                let affected =
                    ApiCollectRepo::resolve_need_service_fee(&funds_pool, &self.trade_no).await?;
                if affected == 0 {
                    tracing::warn!(
                        trade_no = %self.trade_no,
                        trade_type = %self.trade_type,
                        need_service_fee = ?collect.need_service_fee,
                        "FeeRes received but resolve_need_service_fee affected 0 rows"
                    );
                } else {
                    tracing::info!(
                        trade_no = %self.trade_no,
                        trade_type = %self.trade_type,
                        affected = %affected,
                        "Resolved need_service_fee due to FeeRes"
                    );
                }

                // 快速触发一次 Shadow 推进（让 TxFeeResAck / Build / Broadcast 尽快发生）
                if let Some(handles) = ctx.get_global_handles().await.upgrade() {
                    if let Some(shadow_system) =
                        handles.get_global_processed_collect_tx_handle().get_shadow_system()
                    {
                        if let Err(e) = shadow_system.trigger_collect(&self.trade_no).await {
                            tracing::warn!(trade_no=%self.trade_no, "Trigger collect shadow failed after FeeRes, but continuing: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    trade_no = %self.trade_no,
                    trade_type = %self.trade_type,
                    error = %e,
                    "FeeRes received but api_collect trade_no not found"
                );
            }
        }

        Ok(())
    }
}
