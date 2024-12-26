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
    pub resource: ResourceResp,
    pub opration_time: Option<DateTime<Utc>>,
}

impl FreezeListResp {
    pub fn new(resource: ResourceResp) -> Self {
        Self {
            resource,
            opration_time: None,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezeResp {
    pub owner_address: String,
    pub resource: ResourceResp,
    pub votes: i64,
    pub bill_kind: BillKind,
    pub tx_hash: String,
    pub expiration_at: Option<DateTime<Utc>>,
}
impl FreezeResp {
    pub fn new(
        owner_address: String,
        resource: ResourceResp,
        tx_hash: String,
        bill_kind: BillKind,
    ) -> Self {
        Self {
            owner_address,
            votes: resource.amount,
            resource,
            tx_hash,
            bill_kind,
            expiration_at: None,
        }
    }

    pub fn expiration_at(mut self, date: DateTime<Utc>) -> Self {
        self.expiration_at = Some(date);
        self
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawUnfreezeResp {
    pub amount: i64,
    pub owner_address: String,
    pub tx_hash: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelAllUnFreezeResp {
    pub owner_address: String,
    pub votes: i64,
    pub energy: f64,
    pub bandwidth: f64,
    pub amount: i64,
    pub tx_hash: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnfreezeListResp {
    pub amount: i64,
    pub resource_type: ResourceType,
    pub available_at: DateTime<Utc>,
}
impl UnfreezeListResp {
    pub fn new(amount: i64, resource_type: ResourceType, available_at: i64) -> Self {
        let time = DateTime::from_timestamp_millis(available_at).unwrap_or_default();
        Self {
            amount: amount / consts::TRX_TO_SUN as i64,
            resource_type,
            available_at: time,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrxToResourceResp {
    pub resource: ResourceResp,
    // 能够得到的投票数量
    pub votes: i64,
    // 预计转账次数
    pub transfer_times: f64,
}
impl TrxToResourceResp {
    pub fn new(resource: ResourceResp, consumer: f64) -> Self {
        Self {
            transfer_times: (resource.amount as f64 / consumer).floor(),
            votes: resource.amount,
            resource,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceToTrxResp {
    pub amount: i64,
    pub votes: i64,
    pub transfer_times: f64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateResp {
    pub owner_address: String,
    pub receiver_address: String,
    pub resource: ResourceResp,
    pub bill_kind: BillKind,
    pub tx_hash: String,
}
impl DelegateResp {
    pub fn new_delegate(
        req: DelegateReq,
        resource: ResourceResp,
        bill_kind: BillKind,
        tx_hash: String,
    ) -> Self {
        Self {
            owner_address: req.owner_address.to_string(),
            receiver_address: req.receiver_address.to_string(),
            resource,
            bill_kind,
            tx_hash,
        }
    }

    pub fn new_undelegate(
        req: UnDelegateReq,
        resource: ResourceResp,
        bill_kind: BillKind,
        tx_hash: String,
    ) -> Self {
        Self {
            owner_address: req.owner_address.to_string(),
            receiver_address: req.receiver_address.to_string(),
            resource,
            bill_kind,
            tx_hash,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateListResp {
    pub from: String,
    pub to: String,
    pub resource: ResourceResp,
    pub expire_time: Option<DateTime<Utc>>,
}

impl DelegateListResp {
    // acount unit is sun
    pub fn new(
        delegate: &DelegateResouce,
        resource: ResourceResp,
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
            resource,
            expire_time,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDelegateResp {
    pub owner_address: String,
    pub result: Vec<BatchRes>,
    pub resource: ResourceResp,
    pub bill_kind: BillKind,
    pub hashs: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRes {
    pub address: String,
    pub status: bool,
}

impl BatchDelegateResp {
    pub fn new(
        owner_address: String,
        res: (Vec<BatchRes>, Vec<String>),
        resource: ResourceResp,
        bill_kind: BillKind,
    ) -> Self {
        Self {
            owner_address,
            result: res.0,
            resource,
            bill_kind,
            hashs: res.1,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteListResp {
    pub total: u16,
    pub total_votes: i64,
    pub data: Vec<Witness>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Witness {
    pub address: String,
    pub vote_count: i64,
    pub url: String,
    pub brokerage: f64,
    pub apr: f64,
}

impl Witness {
    pub fn new(address: &str, vote_count: i64, url: &str, brokerage: f64, apr: f64) -> Self {
        Self {
            address: address.to_string(),
            vote_count,
            url: url.to_string(),
            brokerage,
            apr,
        }
    }
}
