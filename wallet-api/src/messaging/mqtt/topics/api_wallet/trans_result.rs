use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
};
use wallet_database::repositories::api_wallet::wallet::ApiWalletRepo;

use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

// biz_type = AWM_ORDER_TRANS_RES
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransResMsg {
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    trade_type: u32,
    /// 交易结果： true 成功 /false 失败
    status: bool,
    /// 订单结失败型：0 默认，无意义 /  1 交易正常失败 / 2 手续费失败
    fail_type: Option<i32>,
    uid: String,
}

// API钱包的订单结果消息
impl AwmOrderTransResMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        self.check_uid().await?;
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;
        let data = NotifyEvent::AwmOrderTransRes(self.to_owned());
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let res = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;
        match res {
            Some(_res) => {
                let fail_type = if let Some(ft) = self.fail_type { ft } else { 0 };
                match self.trade_type {
                    1 => self.withdraw().await?,
                    2 => self.collect(fail_type).await?,
                    3 => self.transfer_fee().await?,
                    _ => {}
                }
            }
            None => {}
        }
        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        ApiFeeDomain::confirm_tx(&self.trade_no, self.status).await?;
        Ok(())
    }

    pub(crate) async fn collect(
        &self,
        fail_type: i32,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiCollectDomain::confirm_tx(&self.trade_no, self.status, fail_type).await?;
        Ok(())
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::error::service::ServiceError> {
        let res = ApiWithdrawDomain::confirm_tx(&self.trade_no, self.status).await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!(" ======= trans result {}", e);
                Err(e)
            }
        }
    }
}
