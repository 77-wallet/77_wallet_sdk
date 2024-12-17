use sqlx::types::chrono::{DateTime, Utc};
use wallet_chain_interact::tron::{consts, operations::stake::ResourceType};

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
