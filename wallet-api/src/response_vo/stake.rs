use crate::request::stake::{DelegateReq, UnDelegateReq};
use sqlx::types::chrono::{DateTime, Utc};
use wallet_chain_interact::tron::{
    consts,
    operations::stake::{DelegateResouce, ResourceType},
};
use wallet_database::entities::bill::BillKind;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceResp {
    // trx
    pub amount: i64,
    // energy bandwidth
    pub resource_type: ResourceType,
    // 对应资源的数量
    pub resource_value: f64,
}
impl ResourceResp {
    pub fn new(amount: i64, resource_type: ResourceType, resource_value: f64) -> Self {
        Self {
            amount,
            resource_type,
            resource_value,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezeListResp {
    pub amount: i64,
    pub resource: String,
    pub resource_value: f64,
    pub opration_time: Option<DateTime<Utc>>,
}

impl FreezeListResp {
    // parameter unit is sun
    pub fn new(amount: i64, price: f64, resource: ResourceType) -> Self {
        let resource_value = (amount as f64 * price) / consts::TRX_TO_SUN as f64;
        let resource_value = (resource_value * 100.0).round() / 100.0;

        Self {
            amount: amount / consts::TRX_TO_SUN as i64,
            resource: resource.to_string(),
            resource_value,
            opration_time: None,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezeResp {
    pub resource: ResourceResp,
    pub votes: i64,
    pub bill_kind: BillKind,
    pub tx_hash: String,
    pub expiration_at: Option<DateTime<Utc>>,
}
impl FreezeResp {
    pub fn new(resource: ResourceResp, tx_hash: String, bill_kind: BillKind) -> Self {
        Self {
            votes: resource.amount,
            resource,
            tx_hash,
            bill_kind,
            expiration_at: None,
        }
    }

    pub fn expiration_at(mut self, timestamp: i64) -> Self {
        let time = DateTime::from_timestamp_millis(timestamp).unwrap_or_default();
        self.expiration_at = Some(time);
        self
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawUnfreezeResp {
    pub amount: i64,
    pub tx_hash: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnfreezeListResp {
    pub amount: i64,
    pub resource: String,
    pub available_at: DateTime<Utc>,
}
impl UnfreezeListResp {
    pub fn new(amount: i64, resource: ResourceType, available_at: i64) -> Self {
        let time = DateTime::from_timestamp_millis(available_at).unwrap_or_default();
        Self {
            amount: amount / consts::TRX_TO_SUN as i64,
            resource: resource.to_string(),
            available_at: time,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimatedResourcesResp {
    // 能够获取到的资源
    pub resource: f64,
    // 能够得到的投票数量
    pub votes: i64,
    // 资源类型
    pub resource_type: String,
    // 预计转账次数
    pub transfer_times: f64,
}
impl EstimatedResourcesResp {
    pub fn new(value: i64, price: f64, resource_type: ResourceType, consumer: f64) -> Self {
        let resource = (value as f64 * price * 100.0).round() / 100.0;

        Self {
            resource,
            votes: value,
            resource_type: resource_type.to_string(),
            transfer_times: (resource / consumer).floor(),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanDelegatedResp {
    pub amount: f64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateResp {
    pub owner_address: String,
    pub receiver_address: String,
    pub resource_value: f64,
    pub resource_type: String,
    pub operation_type: &'static str,
    pub tx_hash: String,
}
impl DelegateResp {
    pub fn new_with_delegate(
        req: DelegateReq,
        resource_value: f64,
        resource_type: ResourceType,
        tx_hash: String,
    ) -> Self {
        Self {
            owner_address: req.owner_address.to_string(),
            receiver_address: req.receiver_address.to_string(),
            resource_value,
            resource_type: resource_type.to_string(),
            operation_type: "delegate",
            tx_hash,
        }
    }

    pub fn new_with_undegate(
        req: UnDelegateReq,
        resource_value: f64,
        resource_type: ResourceType,
        tx_hash: String,
    ) -> Self {
        Self {
            owner_address: req.owner_address.to_string(),
            receiver_address: req.receiver_address.to_string(),
            resource_value,
            resource_type: resource_type.to_string(),
            operation_type: "un_delegate",
            tx_hash,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateListResp {
    pub from: String,
    pub to: String,
    // trx 数量
    pub amount: i64,
    // 可获得资源数量
    pub resource_value: f64,
    // 资源类型
    pub resource_type: String,
    pub expire_time: Option<DateTime<Utc>>,
}

impl DelegateListResp {
    // acount unit is sun
    pub fn new(
        delegate: &DelegateResouce,
        resource_value: f64,
        resource_type: ResourceType,
        amount: i64,
        expire_time: i64,
    ) -> Result<DelegateListResp, crate::ServiceError> {
        let expire_time = if expire_time > 0 {
            DateTime::from_timestamp_millis(expire_time)
        } else {
            None
        };

        Ok(DelegateListResp {
            from: delegate.from.to_string(),
            to: delegate.to.to_string(),
            amount,
            resource_value,
            resource_type: resource_type.to_string(),
            expire_time,
        })
    }
}
