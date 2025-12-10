use wallet_database::repositories::api_wallet::wallet::ApiWalletRepo;
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    request::api_wallet::trans::{ApiCollectReq, ApiTransferFeeReq, ApiWithdrawReq},
};

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransMsg {
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "tokenAddr")]
    pub token_address: String,
    #[serde(rename = "tokenCode")]
    pub symbol: String,
    /// 平台交易单号
    pub trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub trade_type: u32,
    /// 是否需要审核（可空）： 1 不需要审核 / 2 需要审核
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub audit: u32,
    pub uid: String,
    validate: String,
}

// 归集和提币
impl AwmOrderTransMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        self.check_uid().await?;
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        let res = backend.msg_ack(msg_ack_req).await;
        match res {
            Ok(_res) => Ok(()),
            Err(e) => {
                tracing::error!("transfer from {} to {} value {:?}", self.from, self.to, &e);
                Err(e.into())
            }
        }
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let res = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?;
        match res {
            Some(_res) => match self.trade_type {
                1 => self.withdraw().await?,
                2 => self.collect().await?,
                3 => self.transfer_fee().await?,
                _ => {}
            },
            None => {}
        }
        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        tracing::info!(
            "开始处理手续费交易, trade_no: {}, from: {}, to: {}, value: {}, chain: {}, token: {}, symbol: {}",
            self.trade_no,
            self.from,
            self.to,
            self.value,
            self.chain_code,
            self.token_address,
            self.symbol
        );

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

        tracing::info!("手续费交易请求参数: {:?}", req);
        let result = ApiFeeDomain::transfer_fee(&req).await;

        match &result {
            Ok(_) => tracing::info!("手续费交易处理成功, trade_no: {}", self.trade_no),
            Err(e) => {
                tracing::error!("手续费交易处理失败, trade_no: {}, error: {:?}", self.trade_no, e)
            }
        }

        result
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::error::service::ServiceError> {
        tracing::info!(
            "开始处理归集交易, trade_no: {}, from: {}, to: {}, value: {}, chain: {}, token: {}, symbol: {}",
            self.trade_no,
            self.from,
            self.to,
            self.value,
            self.chain_code,
            self.token_address,
            self.symbol
        );

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

        tracing::info!("归集交易请求参数: {:?}", req);
        let result = ApiCollectDomain::collect_v2(&req).await;

        match &result {
            Ok(_) => tracing::info!("归集交易处理成功, trade_no: {}", self.trade_no),
            Err(e) => {
                tracing::error!("归集交易处理失败, trade_no: {}, error: {:?}", self.trade_no, e)
            }
        }

        result
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
