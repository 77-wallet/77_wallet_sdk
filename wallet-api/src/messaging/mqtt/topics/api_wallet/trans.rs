// messaging/mqtt/topics/api_wallet/trans.rs
use wallet_database::entities::asset_token_key::AssetTokenKey;
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
    /// 0 默认值，无意义 1 正常地址 2 风险地址； 归集交易，表示from地址是否为风险地址；提笔订单，表示to地址是否为风险地址
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    risk_addr: u32,
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

        self.check_uid().await?;
        Ok(())
    }

    pub(crate) async fn check_uid(&self) -> Result<(), crate::error::service::ServiceError> {
        // 根据强约束原则，只使用输入字段原值，不查询数据库
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            3 => self.transfer_fee().await?,
            _ => {}
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

        let token_address = AssetTokenKey::from_raw(Some(self.token_address.as_str()));
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

        tracing::info!(
            trade_no = %req.trade_no,
            uid = %req.uid,
            trade_type = %req.trade_type,
            "手续费交易请求已构建"
        );
        let result = ApiFeeDomain::transfer_fee(&req).await;

        match &result {
            Ok(_) => {
                tracing::info!("手续费交易发送成功, trade_no: {}", self.trade_no);
            }
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

        let token_address = AssetTokenKey::from_raw(Some(self.token_address.as_str()));
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
            risk_addr: self.risk_addr as u8,
        };

        tracing::info!(
            trade_no = %req.trade_no,
            uid = %req.uid,
            trade_type = %req.trade_type,
            risk_addr = %req.risk_addr,
            "归集交易请求已构建"
        );
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

        let token_address = AssetTokenKey::from_raw(Some(self.token_address.as_str()));
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

#[cfg(test)]
mod tests {
    use super::AwmOrderTransMsg;
    use crate::test::{
        env::get_manager,
        mqtt::{api_transaction_pool, api_wallet_pool},
    };
    use serial_test::serial;
    use tokio::sync::mpsc;
    use wallet_database::{
        entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
        repositories::api_wallet::{
            collect::ApiCollectRepo, fee::ApiFeeRepo, wallet::ApiWalletRepo,
        },
    };

    #[tokio::test]
    #[serial]
    async fn transfer_fee_does_not_touch_collect_row_by_fee_trade_no() -> anyhow::Result<()> {
        let (manager, _params) = get_manager().await?;
        let (frontend_tx, _frontend_rx) = mpsc::unbounded_channel();
        manager.set_frontend_notify_sender(frontend_tx).await?;

        let wallet_uid =
            format!("fee-order-regression-{}", wallet_utils::time::now().timestamp_millis());
        let trade_no =
            format!("CF_fee_order_regression_{}", wallet_utils::time::now().timestamp_millis());

        let wallet_pool = api_wallet_pool()?;
        let seed_enc: Vec<u8> =
            crate::domain::api_wallet::wallet::ApiWalletDomain::encrypt_seed_bundle(
                "q1111111",
                b"test-seed",
            )
            .await
            .unwrap();
        ApiWalletRepo::upsert(
            &wallet_pool,
            &wallet_uid,
            "wallet_name",
            "0x1111111111111111111111111111111111111111",
            b"test-phrase",
            &seed_enc,
            ApiWalletType::SubAccount,
            None,
            "test-sn",
        )
        .await?;

        let tx_pool = api_transaction_pool()?;
        ApiCollectRepo::upsert_api_collect(
            &tx_pool,
            &wallet_uid,
            "collect-wallet",
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222",
            "0.123",
            "digest",
            "eth",
            None::<String>,
            "ETH",
            &trade_no,
            2,
            ApiCollectStatus::Init,
            1,
        )
        .await?;

        let msg = AwmOrderTransMsg {
            from: "0x1111111111111111111111111111111111111111".to_string(),
            to: "0x2222222222222222222222222222222222222222".to_string(),
            value: "0.123".to_string(),
            chain_code: "eth".to_string(),
            token_address: String::new(),
            symbol: "ETH".to_string(),
            trade_no: trade_no.clone(),
            trade_type: 3,
            audit: 1,
            uid: wallet_uid.clone(),
            validate: "digest".to_string(),
            risk_addr: 1,
        };

        msg.transfer_fee().await?;

        let collect = ApiCollectRepo::get_api_collect_by_trade_no(&tx_pool, &trade_no).await?;
        assert!(
            collect.service_fee_order_received_at.is_none(),
            "fee-order side effect must not write collect-side fee order facts"
        );

        let fee = ApiFeeRepo::get_api_fee_by_trade_no(&tx_pool, &trade_no).await?;
        assert_eq!(fee.trade_no, trade_no);

        Ok(())
    }
}
