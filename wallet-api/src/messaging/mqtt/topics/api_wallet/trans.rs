// messaging/mqtt/topics/api_wallet/trans.rs
use wallet_database::entities::asset_token_key::AssetTokenKey;
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::api_wallet::trans::{
        collect::ApiCollectDomain, fee::ApiFeeDomain, withdraw::ApiWithdrawDomain,
    },
    request::api_wallet::trans::{ApiCollectReq, ApiTransferFeeReq, ApiWithdrawReq},
};
use wallet_database::{
    entities::{
        api_resource_delegation::NewApiResourceDelegation,
        api_resource_operation::{ApiResourceOperationType, NewApiResourceOperation},
        api_resource_type::ApiResourceType,
        api_trade_type::ApiTradeType,
    },
    repositories::api_wallet::{
        resource_delegation::ApiResourceDelegationRepo,
        resource_operation::ApiResourceOperationRepo,
    },
};

// biz_type = AWM_ORDER_TRANS
#[derive(Debug, serde::Serialize, Clone)]
#[serde(untagged)]
pub enum AwmOrderTransMsg {
    Normal(AwmOrderTransNormalMsg),
    ResourceOperation(AwmResourceOperationMsg),
    ResourceDelegation(AwmResourceDelegationMsg),
}

impl<'de> serde::Deserialize<'de> for AwmOrderTransMsg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let trade_type = value
            .get("tradeType")
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .or_else(|| x.as_u64().map(|n| n as u32))
            })
            .ok_or_else(|| serde::de::Error::custom("missing tradeType"))?;

        match trade_type {
            4 => serde_json::from_value(value)
                .map(Self::ResourceOperation)
                .map_err(serde::de::Error::custom),
            5 | 7 => serde_json::from_value(value)
                .map(Self::ResourceDelegation)
                .map_err(serde::de::Error::custom),
            _ => serde_json::from_value(value).map(Self::Normal).map_err(serde::de::Error::custom),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransNormalMsg {
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "tokenAddr")]
    #[serde(default)]
    pub token_address: String,
    #[serde(rename = "tokenCode")]
    #[serde(default)]
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
        default,
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub audit: u32,
    pub uid: String,
    #[serde(default)]
    validate: String,
    /// 0 默认值，无意义 1 正常地址 2 风险地址； 归集交易，表示from地址是否为风险地址；提笔订单，表示to地址是否为风险地址
    #[serde(
        default,
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    risk_addr: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmResourceOperationMsg {
    pub from: String,
    pub value: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(
        rename = "rscType",
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    rsc_type: u32,
    #[serde(
        rename = "stkType",
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    stk_type: u32,
    pub trade_no: String,
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub trade_type: u32,
    pub uid: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmResourceDelegationMsg {
    pub from: String,
    pub to: String,
    #[serde(rename = "nativeValue")]
    native_value: String,
    #[serde(rename = "rscValue")]
    rsc_value: String,
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    mode: u32,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(
        rename = "rscType",
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    rsc_type: u32,
    pub trade_no: String,
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub trade_type: u32,
    pub uid: String,
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
        match self {
            Self::Normal(msg) => match msg.trade_type {
                1 => msg.withdraw().await?,
                2 => msg.collect().await?,
                3 => msg.transfer_fee().await?,
                _ => {}
            },
            Self::ResourceOperation(msg) => msg.resource_operation().await?,
            Self::ResourceDelegation(msg) => msg.resource_delegation().await?,
        }
        Ok(())
    }

    pub(crate) async fn resource_operation(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        match self {
            Self::ResourceOperation(msg) => msg.resource_operation().await,
            _ => Ok(()),
        }
    }

    pub(crate) async fn resource_delegation(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        match self {
            Self::ResourceDelegation(msg) => msg.resource_delegation().await,
            _ => Ok(()),
        }
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        match self {
            Self::Normal(msg) => msg.transfer_fee().await,
            _ => Ok(()),
        }
    }

    pub fn from_addr(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.from,
            Self::ResourceOperation(msg) => &msg.from,
            Self::ResourceDelegation(msg) => &msg.from,
        }
    }

    pub fn to_addr(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.to,
            Self::ResourceOperation(_) => "",
            Self::ResourceDelegation(msg) => &msg.to,
        }
    }

    pub fn value(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.value,
            Self::ResourceOperation(msg) => &msg.value,
            Self::ResourceDelegation(msg) => &msg.rsc_value,
        }
    }

    pub fn chain_code(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.chain_code,
            Self::ResourceOperation(msg) => &msg.chain_code,
            Self::ResourceDelegation(msg) => &msg.chain_code,
        }
    }

    pub fn token_address(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.token_address,
            _ => "",
        }
    }

    pub fn symbol(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.symbol,
            _ => "",
        }
    }

    pub fn trade_no(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.trade_no,
            Self::ResourceOperation(msg) => &msg.trade_no,
            Self::ResourceDelegation(msg) => &msg.trade_no,
        }
    }

    pub fn trade_type(&self) -> u32 {
        match self {
            Self::Normal(msg) => msg.trade_type,
            Self::ResourceOperation(msg) => msg.trade_type,
            Self::ResourceDelegation(msg) => msg.trade_type,
        }
    }

    pub fn audit(&self) -> u32 {
        match self {
            Self::Normal(msg) => msg.audit,
            _ => 0,
        }
    }

    pub fn uid(&self) -> &str {
        match self {
            Self::Normal(msg) => &msg.uid,
            Self::ResourceOperation(msg) => &msg.uid,
            Self::ResourceDelegation(msg) => &msg.uid,
        }
    }
}

impl AwmResourceOperationMsg {
    pub(crate) async fn resource_operation(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let operation_type = match self.stk_type {
            1 => ApiResourceOperationType::Stake,
            2 => ApiResourceOperationType::Unstake,
            other => {
                tracing::warn!(
                    trade_no = %self.trade_no,
                    stk_type = %other,
                    "Unknown resource operation stkType, defaulting to stake"
                );
                ApiResourceOperationType::Stake
            }
        };
        let resource_type =
            ApiResourceType::from_backend_rsc_type(self.rsc_type).unwrap_or_else(|| {
                tracing::warn!(
                    trade_no = %self.trade_no,
                    rsc_type = %self.rsc_type,
                    "Unknown resource operation rscType, defaulting to energy"
                );
                ApiResourceType::Energy
            });
        let req = NewApiResourceOperation::backend(
            self.uid.to_string(),
            self.trade_no.to_string(),
            self.from.to_string(),
            resource_type,
            self.value.to_string(),
            operation_type,
        );

        let pool = crate::context::CONTEXT.get().unwrap().api_transaction_pool()?;
        ApiResourceOperationRepo::upsert(&pool, req).await?;
        tracing::info!(
            trade_no = %self.trade_no,
            uid = %self.uid,
            rsc_type = %self.rsc_type,
            stk_type = %self.stk_type,
            "平台资源质押/解锁任务已落库，等待任务 ACK 扫描"
        );
        Ok(())
    }
}

impl AwmResourceDelegationMsg {
    pub(crate) async fn resource_delegation(
        &self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let origin_trade_type = match self.trade_type {
            5 => ApiTradeType::Collect,
            7 => ApiTradeType::Withdraw,
            _ => return Ok(()),
        };
        let resource_type =
            ApiResourceType::from_backend_rsc_type(self.rsc_type).unwrap_or_else(|| {
                tracing::warn!(
                    trade_no = %self.trade_no,
                    rsc_type = %self.rsc_type,
                    "Unknown resource delegation rscType, defaulting to energy"
                );
                ApiResourceType::Energy
            });
        let amount = self.resource_delegation_amount();
        let req = NewApiResourceDelegation::platform_delegate_task(
            self.uid.to_string(),
            self.trade_no.to_string(),
            origin_trade_type,
            self.chain_code.to_string(),
            self.from.to_string(),
            self.to.to_string(),
            resource_type,
            amount,
        );

        let pool = crate::context::CONTEXT.get().unwrap().api_transaction_pool()?;
        ApiResourceDelegationRepo::upsert(&pool, req).await?;
        tracing::info!(
            trade_no = %self.trade_no,
            uid = %self.uid,
            trade_type = %self.trade_type,
            rsc_type = %self.rsc_type,
            mode = %self.mode,
            "平台资源代理任务已落库，等待任务 ACK 扫描"
        );
        Ok(())
    }

    fn resource_delegation_amount(&self) -> String {
        if !self.rsc_value.trim().is_empty() {
            return self.rsc_value.clone();
        }
        if !self.native_value.trim().is_empty() {
            return self.native_value.clone();
        }
        self.native_value.clone()
    }
}

impl AwmOrderTransNormalMsg {
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
    use super::{
        AwmOrderTransMsg, AwmOrderTransNormalMsg, AwmResourceDelegationMsg, AwmResourceOperationMsg,
    };
    use crate::test::{
        env::get_manager,
        mqtt::{api_transaction_pool, api_wallet_pool},
    };
    use serial_test::serial;
    use tokio::sync::mpsc;
    use wallet_database::{
        entities::{
            api_collect::ApiCollectStatus,
            api_resource_delegation::{
                ApiResourceDelegationOperationType, ApiResourceDelegationSource,
            },
            api_resource_operation::{ApiResourceOperationTaskSource, ApiResourceOperationType},
            api_resource_type::ApiResourceType,
            api_trade_type::ApiTradeType,
            api_wallet::ApiWalletType,
        },
        repositories::api_wallet::{
            collect::ApiCollectRepo, fee::ApiFeeRepo,
            resource_delegation::ApiResourceDelegationRepo,
            resource_operation::ApiResourceOperationRepo, wallet::ApiWalletRepo,
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
            0,
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

        let msg = AwmOrderTransMsg::Normal(AwmOrderTransNormalMsg {
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
        });

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

    #[tokio::test]
    #[serial]
    async fn resource_operation_trade_type_4_persists_backend_stake_task() -> anyhow::Result<()> {
        let (_manager, _params) = get_manager().await?;
        let trade_no =
            format!("RSC_STK_order_regression_{}", wallet_utils::time::now().timestamp_millis());
        let wallet_uid =
            format!("rsc-stk-order-regression-{}", wallet_utils::time::now().timestamp_millis());

        let msg = AwmOrderTransMsg::ResourceOperation(AwmResourceOperationMsg {
            from: "T_resource_owner".to_string(),
            value: "1000".to_string(),
            chain_code: "tron".to_string(),
            rsc_type: 1,
            stk_type: 1,
            trade_no: trade_no.clone(),
            trade_type: 4,
            uid: wallet_uid,
        });

        msg.resource_operation().await?;

        let tx_pool = api_transaction_pool()?;
        let got = ApiResourceOperationRepo::get_by_resource_trade_no(&tx_pool, &trade_no).await?;
        assert_eq!(got.task_source, ApiResourceOperationTaskSource::Backend);
        assert_eq!(got.operation_type, ApiResourceOperationType::Stake);
        assert_eq!(got.resource_type, ApiResourceType::Energy);
        assert_eq!(got.owner_address, "T_resource_owner");
        assert_eq!(got.amount, "1000");
        assert!(got.task_ack_sent_at.is_none());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn resource_delegation_trade_type_5_persists_collect_platform_delegate()
    -> anyhow::Result<()> {
        let (_manager, _params) = get_manager().await?;
        let trade_no =
            format!("COL_RSC_DL_order_regression_{}", wallet_utils::time::now().timestamp_millis());
        let wallet_uid =
            format!("col-rsc-dl-order-regression-{}", wallet_utils::time::now().timestamp_millis());

        let msg = AwmOrderTransMsg::ResourceDelegation(AwmResourceDelegationMsg {
            from: "T_platform_owner".to_string(),
            to: "T_collect_receiver".to_string(),
            native_value: "2".to_string(),
            rsc_value: "32000".to_string(),
            mode: 1,
            chain_code: "tron".to_string(),
            rsc_type: 1,
            trade_no: trade_no.clone(),
            trade_type: 5,
            uid: wallet_uid.clone(),
        });

        msg.resource_delegation().await?;

        let tx_pool = api_transaction_pool()?;
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&tx_pool, &trade_no).await?;
        assert_eq!(got.uid, wallet_uid);
        assert_eq!(got.source, ApiResourceDelegationSource::Platform);
        assert_eq!(got.operation_type, ApiResourceDelegationOperationType::Delegate);
        assert_eq!(got.origin_trade_type, Some(ApiTradeType::Collect as i64));
        assert_eq!(got.origin_trade_no, None);
        assert_eq!(got.owner_address, "T_platform_owner");
        assert_eq!(got.receiver_address, "T_collect_receiver");
        assert_eq!(got.resource_type, ApiResourceType::Energy);
        assert_eq!(got.amount, "32000");
        assert!(got.task_ack_sent_at.is_none());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn resource_delegation_trade_type_7_persists_withdraw_platform_delegate()
    -> anyhow::Result<()> {
        let (_manager, _params) = get_manager().await?;
        let trade_no =
            format!("WD_RSC_DL_order_regression_{}", wallet_utils::time::now().timestamp_millis());
        let wallet_uid =
            format!("wd-rsc-dl-order-regression-{}", wallet_utils::time::now().timestamp_millis());

        let msg = AwmOrderTransMsg::ResourceDelegation(AwmResourceDelegationMsg {
            from: "T_platform_owner".to_string(),
            to: "T_withdraw_receiver".to_string(),
            native_value: "3".to_string(),
            rsc_value: "64000".to_string(),
            mode: 2,
            chain_code: "tron".to_string(),
            rsc_type: 1,
            trade_no: trade_no.clone(),
            trade_type: 7,
            uid: wallet_uid,
        });

        msg.resource_delegation().await?;

        let tx_pool = api_transaction_pool()?;
        let got = ApiResourceDelegationRepo::get_by_resource_trade_no(&tx_pool, &trade_no).await?;
        assert_eq!(got.origin_trade_type, Some(ApiTradeType::Withdraw as i64));
        assert_eq!(got.source, ApiResourceDelegationSource::Platform);
        assert_eq!(got.operation_type, ApiResourceDelegationOperationType::Delegate);
        assert_eq!(got.owner_address, "T_platform_owner");
        assert_eq!(got.receiver_address, "T_withdraw_receiver");
        assert_eq!(got.amount, "64000");

        assert!(
            ApiResourceOperationRepo::get_by_resource_trade_no(&tx_pool, &trade_no).await.is_err(),
            "tradeType=7 resource delegation must not write api_resource_operation"
        );

        Ok(())
    }
}
