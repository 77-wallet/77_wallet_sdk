use wallet_database::repositories::api_wallet::wallet::ApiWalletRepo;
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::api_wallet::{
        trans::{collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain},
        wallet::ApiWalletDomain,
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
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        backend.msg_ack(msg_ack_req).await?;

        let _password = ApiWalletDomain::get_passwd().await?;

        // 在MQTT消息收到时获取并存储私钥到私钥管理器
        if self.trade_type == 2 || self.trade_type == 3 {
            // 2: 归集, 3: 归集手续费交易
            tracing::info!(
                "MQTT消息收到, 获取并存储私钥, trade_no: {}, from: {}, chain_code: {}",
                self.trade_no,
                self.from,
                self.chain_code
            );

            // 通过Context获取Handles实例，然后获取私钥管理器
            let handles = crate::context::get_context()?.get_handles_arc().await?;
            let private_key_manager = handles.get_global_private_key_manager();
            match private_key_manager.preload(&self.from, &self.chain_code).await {
                Ok(_) => {
                    tracing::info!("私钥预加载指令已发送, trade_no: {}", self.trade_no);
                }
                Err(e) => {
                    tracing::error!(
                        "私钥预加载指令发送失败, trade_no: {}, error: {:?}",
                        self.trade_no,
                        e
                    );
                }
            }
        }

        self.check_uid().await?;
        Ok(())
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
            Ok(_) => tracing::info!("手续费交易发送成功, trade_no: {}", self.trade_no),
            Err(e) => {
                tracing::error!("手续费交易发送失败, trade_no: {}, error: {:?}", self.trade_no, e)
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
