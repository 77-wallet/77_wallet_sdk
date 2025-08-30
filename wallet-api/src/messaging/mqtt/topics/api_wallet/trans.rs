use crate::{
    domain::{
        api_wallet::{collect::ApiCollectDomain, withdraw::ApiWithdrawDomain},
        chain::transaction::ChainTransDomain,
    },
    request::{
        api_wallet::trans::ApiWithdrawReq,
        transaction::{BaseTransferReq, TransferReq},
    },
    service::transaction::TransactionService,
};
use wallet_database::{
    entities::{api_wallet::ApiWalletType, assets::AssetsId},
    repositories::{
        api_assets::ApiAssetsRepo, api_collect::ApiCollectRepo, api_wallet::ApiWalletRepo,
    },
};
use wallet_utils::conversion;

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransMsg {
    from: String,
    to: String,
    value: String,
    #[serde(rename = "chain")]
    chain_code: String,
    #[serde(rename = "token_addr")]
    token_address: String,
    #[serde(rename = "token_code")]
    symbol: String,
    trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    trade_type: u8,
    uid: String,
}

// 归集和提币
impl TransMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::ServiceError> {
        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiWithdrawReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type,
        };
        ApiCollectDomain::collect(&req).await
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiWithdrawReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type,
        };
        ApiWithdrawDomain::withdraw(&req).await
    }
}
