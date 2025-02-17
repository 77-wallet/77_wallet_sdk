use wallet_database::entities::bill::BillKind;

use crate::mqtt::payload::incoming::resource::{TronSignFreezeDelegateVoteChange, Vote};

// biz_type = TRON_SIGN_FREEZE_DELEGATE_VOTE_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TronSignFreezeDelegateVoteChangeFrontend {
    pub tx_hash: String,
    pub chain_code: String,
    pub symbol: String,
    pub transfer_type: i8,
    pub tx_kind: BillKind,
    pub from_addr: String,
    pub to_addr: String,
    pub token: Option<String>,
    pub value: f64,
    pub value_usdt: f64,
    pub transaction_fee: f64,
    pub transaction_time: String,
    pub status: bool,
    pub is_multisig: i32,
    pub block_height: i64,
    pub notes: String,
    pub queue_id: String,
    pub net_used: f64,
    pub energy_used: f64,
    pub resource: String,
    pub lock: bool,
    pub lock_period: String,
    pub votes: Vec<Vote>,
}

impl From<TronSignFreezeDelegateVoteChange> for TronSignFreezeDelegateVoteChangeFrontend {
    fn from(value: TronSignFreezeDelegateVoteChange) -> Self {
        Self {
            tx_hash: value.tx_hash,
            chain_code: value.chain_code,
            symbol: value.symbol,
            transfer_type: value.transfer_type,
            tx_kind: value.tx_kind,
            from_addr: value.from_addr,
            to_addr: value.to_addr,
            token: value.token,
            value: value.value,
            value_usdt: value.value_usdt,
            transaction_fee: value.transaction_fee,
            transaction_time: value.transaction_time,
            status: value.status,
            is_multisig: value.is_multisig,
            block_height: value.block_height,
            notes: value.notes,
            queue_id: value.queue_id,
            net_used: value.net_used,
            energy_used: value.energy_used,
            resource: value.resource,
            lock: value.lock,
            lock_period: value.lock_period,
            votes: value.votes,
        }
    }
}
