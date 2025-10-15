use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::{
        api_wallet::trans::{
            collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
        },
    },
    request::api_wallet::trans::{
        ApiBaseTransferReq, ApiCollectReq, ApiTransferFeeReq, ApiTransferReq, ApiWithdrawReq,
    },
};

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransMsg {
    from: String,
    to: String,
    value: String,
    #[serde(rename = "chain")]
    chain_code: String,
    #[serde(rename = "tokenAddr")]
    token_address: String,
    #[serde(rename = "tokenCode")]
    symbol: String,
    /// 平台交易单号
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    trade_type: u32,
    /// 是否需要审核（可空）： 1 不需要审核 / 2 需要审核
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    audit: u32,
    uid: String,
    validate: String,
}

// 归集和提币
impl AwmOrderTransMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            3 => self.transfer_fee().await?,
            _ => {}
        }
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        let res = backend.msg_ack(msg_ack_req).await;
        match res {
            Ok(res) => {
                Ok(())
            }
            Err(e) => {
                tracing::error!("transfer from {} to {} value {:?}", self.from, self.to, &e);
                Err(e.into())
            }
        }
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiTransferFeeReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            validate: self.validate.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
        };
        ApiFeeDomain::transfer_fee(&req).await
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::error::service::ServiceError> {
        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiCollectReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            validate: self.validate.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
        };
        ApiCollectDomain::collect_v2(&req).await
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码

        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiWithdrawReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            validate: self.validate.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
            audit: self.audit,
        };
        ApiWithdrawDomain::withdraw(&req).await
    }
}
