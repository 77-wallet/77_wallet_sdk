use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;

use crate::{messaging::notify::api_wallet::WithdrawFront, FrontendNotifyEvent, NotifyEvent};

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawMsg {
    pub uid: String,
    pub name: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub decimals: i32,
    pub token_addr: String,
    pub symbol: String,
    pub trade_no: String,
    pub trade_type: String,
    pub status: u8,
}

// 提现
impl WithdrawMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::upsert_api_withdraw(
            &pool,
            &self.uid,
            &self.name,
            &self.from_addr,
            &self.to_addr,
            &self.value,
            &self.token_addr,
            &self.symbol,
            &self.trade_no,
            &self.trade_type,
        )
        .await?;

        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: self.uid.to_string(),
            from_addr: self.from_addr.to_string(),
            to_addr: self.to_addr.to_string(),
            value: self.value.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        // 可能发交易
        let value = Decimal::from_str(&self.value).unwrap();
        if (value < Decimal::from(10)) {
            // 发交易
            // let tx_hash = ChainTransDomain::transfer(params, bill_kind, &adapter).await?;
        }
        Ok(())
    }
}
