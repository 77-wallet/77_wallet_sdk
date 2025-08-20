use super::has_expiration;
use sqlx::types::chrono::{DateTime, Utc};

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiBillEntity {
    pub id: i32,
    pub hash: String,
    pub chain_code: String,
    pub symbol: String,
    // 0转入 1转出
    pub transfer_type: i8,
    // 1:普通交易，2:部署多签账号手续费 3:服务费
    pub tx_kind: ApiBillKind,
    // 订单归属
    pub owner: String,
    pub from_addr: String,
    pub to_addr: String,
    pub token: Option<String>,
    pub value: String,
    // 需要跳过这个字段
    #[serde(skip_serializing)]
    pub resource_consume: String,
    pub transaction_fee: String,
    pub transaction_time: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    // 1-pending 2-成功 3-失败
    pub status: i8,
    // 1-是，0-否
    pub is_multisig: i8,
    pub block_height: String,
    pub queue_id: String,
    pub notes: String,
    pub signer: String,
    // 针对每种订单的额外数据类 默认 “”
    pub extra: String,
    #[serde(skip_serializing)]
    pub created_at:sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

impl ApiBillEntity {
    pub fn get_other_address(&self) -> String {
        if self.transfer_type == 1 {
            self.to_addr.clone()
        } else {
            self.from_addr.clone()
        }
    }

    // Different chains determine failure based on time (if the transaction result is not found when it expires, it is considered a failed transaction)
    pub fn is_failed(&self) -> bool {
        let chain_code =
            wallet_types::chain::chain::ChainCode::try_from(self.chain_code.as_str()).unwrap();

        has_expiration(self.transaction_time.timestamp(), chain_code)
    }

    // 截断金额
    pub fn truncate_to_8_decimals(&mut self) {
        self.value = wallet_utils::unit::truncate_to_8_decimals(&self.value)
    }
}

pub enum ApiBillStatus {
    Pending = 1,
    Success,
    Failed,
}

impl ApiBillStatus {
    pub fn to_i8(&self) -> i8 {
        match self {
            ApiBillStatus::Pending => 1,
            ApiBillStatus::Success => 2,
            ApiBillStatus::Failed => 3,
        }
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    sqlx::Type,
)]
#[repr(u8)]
#[derive(PartialEq)]
pub enum ApiBillKind {
    // 普通交易
    #[default]
    Transfer = 1,
    // 部署多签账号手续费
    DeployMultiSign = 2,
    // 服务费
    ServiceCharge = 3,
    // 多签交易(not used)
    MultiSignTx = 4,
    // 多签交易签名费
    SigningFee = 5,
    // 质押带宽
    FreezeBandwidth = 6,
    // 质押能量
    FreezeEnergy = 7,
    // 解质带宽
    UnFreezeBandwidth = 8,
    // 解质能量
    UnFreezeEnergy = 9,
    // 全部取消质押
    CancelAllUnFreeze = 10,
    // 质押提取
    WithdrawUnFreeze = 11,
    // 委派带宽
    DelegateBandwidth = 12,
    // 委派能量
    DelegateEnergy = 13,
    // 批量委派(带宽)
    BatchDelegateBandwidth = 14,
    // 批量委派(能量)
    BatchDelegateEnergy = 15,
    // 回收带宽
    UnDelegateBandwidth = 16,
    // 回收能量
    UnDelegateEnergy = 17,
    // 批量回收带宽
    BatchUnDelegateBandwidth = 18,
    // 批量回收能量
    BatchUnDelegateEnergy = 19,
    // 投票
    Vote = 20,
    // 奖励提取
    WithdrawReward = 21,
    // 更新权限
    UpdatePermission = 22,
    // 授权交易
    Approve = 23,
    // 授权交易
    UnApprove = 24,
    // swap 交易
    Swap = 25,
}

impl ApiBillKind {
    pub fn to_i8(&self) -> i8 {
        *self as i8
    }

    // 金额是否转出的交易类型(针对本地的交易)
    pub fn out_transfer_type(&self) -> bool {
        matches!(
            self,
            ApiBillKind::Transfer
                | ApiBillKind::ServiceCharge
                | ApiBillKind::MultiSignTx
                | ApiBillKind::SigningFee
                | ApiBillKind::DeployMultiSign
                | ApiBillKind::FreezeBandwidth
                | ApiBillKind::FreezeEnergy
        )
    }

    // 哪些交易类型是转入的的(在freeze中)
    pub fn in_transfer_type(&self) -> bool {
        matches!(
            self,
            ApiBillKind::WithdrawUnFreeze
                | ApiBillKind::WithdrawReward
                | ApiBillKind::Swap
                | ApiBillKind::Approve
                | ApiBillKind::UnApprove
        )
    }

    // 这个交易类型是否需要创建系统通知
    pub fn needs_system_notify(&self) -> bool {
        matches!(self, ApiBillKind::Transfer)
    }
}

impl TryFrom<i8> for ApiBillKind {
    type Error = crate::Error;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ApiBillKind::Transfer),
            2 => Ok(ApiBillKind::DeployMultiSign),
            3 => Ok(ApiBillKind::ServiceCharge),
            4 => Ok(ApiBillKind::MultiSignTx),
            5 => Ok(ApiBillKind::SigningFee),
            6 => Ok(ApiBillKind::FreezeBandwidth),
            7 => Ok(ApiBillKind::FreezeEnergy),
            8 => Ok(ApiBillKind::UnFreezeBandwidth),
            9 => Ok(ApiBillKind::UnFreezeEnergy),
            10 => Ok(ApiBillKind::CancelAllUnFreeze),
            11 => Ok(ApiBillKind::WithdrawUnFreeze),
            12 => Ok(ApiBillKind::DelegateBandwidth),
            13 => Ok(ApiBillKind::DelegateEnergy),
            14 => Ok(ApiBillKind::BatchDelegateBandwidth),
            15 => Ok(ApiBillKind::BatchDelegateEnergy),
            16 => Ok(ApiBillKind::UnDelegateBandwidth),
            17 => Ok(ApiBillKind::UnDelegateEnergy),
            18 => Ok(ApiBillKind::BatchUnDelegateBandwidth),
            19 => Ok(ApiBillKind::BatchUnDelegateEnergy),
            20 => Ok(ApiBillKind::Vote),
            21 => Ok(ApiBillKind::WithdrawReward),
            22 => Ok(ApiBillKind::UpdatePermission),
            23 => Ok(ApiBillKind::Approve),
            24 => Ok(ApiBillKind::UnApprove),
            25 => Ok(ApiBillKind::Swap),
            _ => Err(crate::Error::Other(format!(
                "Invalid value for TxKind : {}",
                value
            ))),
        }
    }
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiRecentBillListVo {
    pub chain_code: String,
    pub symbol: String,
    pub tx_hash: String,
    pub value: String,
    pub address: String,
    pub transaction_time: DateTime<Utc>,
    pub transfer_type: i8,
    pub created_at: DateTime<Utc>,
}

impl From<&ApiBillEntity> for ApiRecentBillListVo {
    fn from(item: &ApiBillEntity) -> Self {
        Self {
            chain_code: item.chain_code.clone(),
            symbol: item.symbol.clone(),
            tx_hash: item.hash.clone(),
            value: item.value.clone(),
            address: item.get_other_address(),
            transaction_time: item.transaction_time,
            transfer_type: item.transfer_type,
            created_at: item.created_at,
        }
    }
}

#[derive(Debug)]
pub struct ApiBillUpdateEntity {
    pub hash: String,
    pub format_fee: String,
    pub transaction_time: u128,
    pub status: i8,
    pub block_height: u128,
    pub resource_consume: String,
}

impl ApiBillUpdateEntity {
    pub fn new(
        hash: String,
        format_fee: String,
        time: u128,
        status: i8,
        block_height: u128,
        resource_consume: String,
    ) -> Self {
        Self {
            hash,
            format_fee,
            transaction_time: time,
            status,
            block_height,
            resource_consume,
        }
    }
}
