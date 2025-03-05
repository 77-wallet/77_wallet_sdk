pub mod acct_change;
pub mod multisign_trans_accept;
pub mod multisign_trans_accept_complete_msg;
pub mod multisign_trans_cancel;
use sqlx::types::chrono::{DateTime, Utc};
use wallet_database::entities::{
    bill::BillKind,
    multisig_queue::{MultisigQueueData, MultisigQueueEntity},
    multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity},
};

// biz_type = MULTI_SIGN_TRANS_ACCEPT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAccept {
    /// queue_id
    pub id: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub expiration: i64,
    #[serde(deserialize_with = "wallet_utils::serde_func::deserialize_uppercase")]
    pub symbol: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub msg_hash: String,
    pub tx_hash: String,
    pub raw_data: String,
    /// 0待签名 1待执行 2已执行
    pub status: i8,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub signatures: Vec<NewSignatureEntity>,
    pub account_id: String,
    pub transfer_type: BillKind,
    pub permission_id: String,
}
impl MultiSignTransAccept {
    pub fn with_signature(mut self, signatures: Vec<NewSignatureEntity>) -> Self {
        self.signatures = signatures;
        self
    }

    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_ACCEPT".to_string()
    }
}

impl TryFrom<MultisigQueueEntity> for MultiSignTransAccept {
    type Error = crate::ServiceError;
    fn try_from(value: MultisigQueueEntity) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            from_addr: value.from_addr,
            to_addr: value.to_addr,
            value: value.value,
            expiration: value.expiration,
            symbol: value.symbol,
            chain_code: value.chain_code,
            token_addr: value.token_addr,
            msg_hash: value.msg_hash,
            tx_hash: value.tx_hash,
            raw_data: value.raw_data,
            status: value.status,
            notes: value.notes,
            created_at: value.created_at,
            signatures: vec![],
            account_id: value.account_id,
            transfer_type: value.transfer_type.try_into()?,
            permission_id: value.permission_id,
        })
    }
}
impl TryFrom<&MultisigQueueData> for MultiSignTransAccept {
    type Error = crate::ServiceError;

    fn try_from(value: &MultisigQueueData) -> Result<Self, Self::Error> {
        let mut signatures = vec![];

        for item in value.signatures.0.iter() {
            let signature = NewSignatureEntity {
                queue_id: value.queue.id.clone(),
                address: item.address.clone(),
                signature: item.signature.clone(),
                status: MultisigSignatureStatus::try_from(item.status as i32)?,
            };
            signatures.push(signature);
        }

        Ok(Self {
            id: value.queue.id.clone(),
            from_addr: value.queue.from_addr.clone(),
            to_addr: value.queue.to_addr.clone(),
            value: value.queue.value.clone(),
            expiration: value.queue.expiration,
            symbol: value.queue.symbol.clone(),
            chain_code: value.queue.chain_code.clone(),
            token_addr: value.queue.token_addr.clone(),
            msg_hash: value.queue.msg_hash.clone(),
            tx_hash: value.queue.tx_hash.clone(),
            raw_data: value.queue.raw_data.clone(),
            status: value.queue.status,
            notes: value.queue.notes.clone(),
            created_at: value.queue.created_at,
            signatures,
            account_id: value.queue.account_id.clone(),
            transfer_type: value.queue.transfer_type.try_into()?,
            permission_id: value.queue.permission_id.clone(),
        })
    }
}

// MULTI_SIGN_TRANS_CANCEL
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransCancel {
    pub withdraw_id: String,
}

impl MultiSignTransCancel {
    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_CANCEL".to_string()
    }
}

// biz_type = MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAcceptCompleteMsg(Vec<MultiSignTransAcceptCompleteMsgBody>);

impl MultiSignTransAcceptCompleteMsg {
    pub(crate) fn name(&self) -> String {
        "MULTI_SIGN_TRANS_ACCEPT_COMPLETE_MSG".to_string()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultiSignTransAcceptCompleteMsgBody {
    pub queue_id: String,
    pub address: String,
    pub signature: String,
    /// 0未签 1签名  2拒绝
    pub status: i8,
}

impl From<&NewSignatureEntity> for MultiSignTransAcceptCompleteMsgBody {
    fn from(value: &NewSignatureEntity) -> Self {
        Self {
            queue_id: value.queue_id.clone(),
            address: value.address.clone(),
            signature: value.signature.clone(),
            status: value.status.to_i8(),
        }
    }
}

// biz_type = ACCT_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AcctChange {
    // 交易hash
    pub tx_hash: String,
    // 链码
    pub chain_code: String,
    // 币种符号
    #[serde(deserialize_with = "wallet_utils::serde_func::deserialize_uppercase")]
    pub symbol: String,
    // 交易方式 0转入 1转出 2初始化
    pub transfer_type: i8,
    // 交易类型 1:普通交易，2:部署多签账号 3:服务费
    pub tx_kind: i8,
    // 发起方
    pub from_addr: String,
    // 接收方
    #[serde(default)]
    pub to_addr: String,
    // 合约地址
    #[serde(default)]
    pub token: Option<String>,
    // 交易额
    #[serde(default)]
    pub value: f64,
    // 手续费
    pub transaction_fee: f64,
    // 交易时间
    #[serde(default)]
    pub transaction_time: String,
    // 交易状态 true-成功 false-失败
    pub status: bool,
    // 是否多签 1-是，0-否
    #[serde(default)]
    pub is_multisig: i32,
    // 队列id
    #[serde(default)]
    pub queue_id: String,
    // 块高
    pub block_height: i64,
    // 备注
    #[serde(default)]
    pub notes: String,
    // 带宽消耗
    #[serde(default)]
    pub net_used: u64,
    // 能量消耗
    #[serde(default)]
    pub energy_used: Option<u64>,
}

impl AcctChange {
    pub(crate) fn name(&self) -> String {
        "ACCT_CHANGE".to_string()
    }
}
