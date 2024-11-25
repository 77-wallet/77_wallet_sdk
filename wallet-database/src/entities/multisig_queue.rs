use super::multisig_signatures::{MultisigSignatureEntities, NewSignatureEntity};
use sqlx::types::chrono::{DateTime, Utc};

pub mod fail_reason {
    pub const SIGN_FAILED: &str = "sign_failed";
    pub const EXPIRED: &str = "expired";
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, Clone)]
pub struct MultisigQueueEntity {
    pub id: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub expiration: i64,
    pub symbol: String,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub msg_hash: String,
    pub tx_hash: String,
    pub raw_data: String,
    /// 0待签名 1待执行 2已执行
    pub status: i8,
    pub notes: String,
    pub fail_reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub account_id: String,
}
impl MultisigQueueEntity {
    pub fn token_address(&self) -> Option<String> {
        self.token_addr
            .as_ref()
            .filter(|token| !token.is_empty())
            .cloned()
    }

    // default status : pending signature
    pub fn compute_status(sign_num: usize, threshold: usize) -> MultisigQueueStatus {
        if sign_num > 0 && sign_num < threshold {
            return MultisigQueueStatus::HasSignature;
        } else if sign_num >= threshold {
            return MultisigQueueStatus::PendingExecution;
        }

        MultisigQueueStatus::PendingSignature
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MultisigQueueStatus {
    PendingSignature = 0, // 待签名
    HasSignature,         // 已签名
    PendingExecution,     // 待执行
    InConfirmation,       // 确认中(交易已经提交，等待链上确认)
    Success,              // 成功
    Fail,                 // 失败
}

impl MultisigQueueStatus {
    pub fn to_i8(&self) -> i8 {
        match self {
            MultisigQueueStatus::PendingSignature => 0,
            MultisigQueueStatus::HasSignature => 1,
            MultisigQueueStatus::PendingExecution => 2,
            MultisigQueueStatus::InConfirmation => 3,
            MultisigQueueStatus::Success => 4,
            MultisigQueueStatus::Fail => 5,
        }
    }

    pub fn from_i8(status: i8) -> MultisigQueueStatus {
        match status {
            0 => MultisigQueueStatus::PendingSignature,
            1 => MultisigQueueStatus::HasSignature,
            2 => MultisigQueueStatus::PendingExecution,
            3 => MultisigQueueStatus::InConfirmation,
            4 => MultisigQueueStatus::Success,
            5 => MultisigQueueStatus::Fail,
            _ => MultisigQueueStatus::PendingSignature,
        }
    }
}

#[derive(Debug, serde::Serialize, sqlx::FromRow, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultisigQueueWithAccountEntity {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub threshold: i32,
    pub member_num: i32,
    #[sqlx(default)]
    pub sign_num: Option<i64>,
    pub owner: i32,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub expiration: i64,
    pub symbol: String,
    pub chain_code: String,
    pub fail_reason: String,
    pub msg_hash: String,
    pub tx_hash: String,
    pub status: i8,
    pub notes: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct NewMultisigQueueEntity {
    pub id: String,
    pub account_id: String,
    pub from_addr: String,
    pub to_addr: String,
    pub value: String,
    pub symbol: String,
    pub expiration: i64,
    pub chain_code: String,
    pub token_addr: Option<String>,
    pub msg_hash: String,
    pub tx_hash: String,
    pub raw_data: String,
    pub status: MultisigQueueStatus,
    pub notes: String,
    pub fail_reason: String,
    pub signatures: Vec<NewSignatureEntity>,
    pub create_at: DateTime<Utc>,
}
impl NewMultisigQueueEntity {
    pub fn with_msg_hash(mut self, msg_hash: &str) -> Self {
        self.msg_hash = msg_hash.to_string();
        self
    }

    pub fn with_raw_data(mut self, raw_data: &str) -> Self {
        self.raw_data = raw_data.to_owned();
        self
    }

    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    pub fn set_id(mut self) -> Self {
        let id = wallet_utils::snowflake::get_uid().unwrap();
        self.id = id.to_string();
        self
    }

    pub fn with_signatures(mut self, signatures: NewSignatureEntity) -> Self {
        self.signatures.push(signatures);
        self
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token_addr = token;
        self
    }

    // recover multisig queue need check status
    pub fn check_expiration(mut self) -> Self {
        if self.needs_expiration_check() && self.is_expired() {
            self.status = MultisigQueueStatus::Fail;
            self.fail_reason = fail_reason::EXPIRED.to_string();
        }
        self
    }

    fn needs_expiration_check(&self) -> bool {
        matches!(
            self.status,
            MultisigQueueStatus::InConfirmation
                | MultisigQueueStatus::HasSignature
                | MultisigQueueStatus::PendingSignature
        ) && self.tx_hash.is_empty()
    }

    fn is_expired(&self) -> bool {
        self.expiration < wallet_utils::time::now().timestamp()
    }
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MemberSignedResult {
    pub name: String,
    pub address: String,
    pub is_self: i8,
    pub singed: i8,
    pub signature: String,
}
impl MemberSignedResult {
    pub fn new(name: &str, address: &str, is_self: i8) -> Self {
        Self {
            name: name.to_string(),
            address: address.to_string(),
            is_self,
            singed: 0,
            signature: "".to_string(),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MultisigQueueData {
    pub queue: MultisigQueueEntity,
    pub signatures: MultisigSignatureEntities,
}
impl MultisigQueueData {
    pub fn new(
        queue: MultisigQueueEntity,
        signatures: MultisigSignatureEntities,
    ) -> MultisigQueueData {
        Self { queue, signatures }
    }

    pub fn to_string(&self) -> Result<String, crate::Error> {
        Ok(wallet_utils::hex_func::bincode_encode(self)?)
    }

    pub fn from_string(data: &str) -> Result<Self, crate::Error> {
        Ok(wallet_utils::hex_func::bincode_decode(data)?)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct QueueTaskEntity {
    pub id: String,
    pub status: i8,
}

impl From<MultisigQueueEntity> for NewMultisigQueueEntity {
    fn from(value: MultisigQueueEntity) -> Self {
        NewMultisigQueueEntity {
            id: value.id,
            account_id: value.account_id,
            from_addr: value.from_addr,
            to_addr: value.to_addr,
            value: value.value,
            symbol: value.symbol,
            expiration: value.expiration,
            chain_code: value.chain_code,
            token_addr: value.token_addr,
            msg_hash: value.msg_hash,
            tx_hash: value.tx_hash,
            raw_data: value.raw_data,
            status: MultisigQueueStatus::from_i8(value.status),
            notes: value.notes,
            fail_reason: value.fail_reason.to_string(),
            signatures: vec![],
            create_at: value.created_at,
        }
    }
}

#[test]
fn test() {
    let str = "12000000000000003139383439323238393130303831323238382200000000000000545262484437375936575744617a395835657372564b77455677524d34675477364e220000000000000054514a53415a6a3454357139424862513148677750484d72643850486838317651650400000000000000302e3031254f416700000000040000000000000055534454040000000000000074726f6e0122000000000000005452374e48716a654b5178475443693871385a5934704c386f74537a676a4c6a367440000000000000003561313139623733393066653363653761376363663232663031396130326132303435613834626535336662393635303862396464336635633032366362356300000000000000008405000000000000514141414141414141414131595445784f5749334d7a6b775a6d557a593255335954646a593259794d6d59774d546c684d444a684d6a41304e5745344e474a6c4e544e6d596a6b324e544134596a6c6b5a444e6d4e574d774d6a5a6a596a566a477749414141414141414237496d4e76626e527959574e30496a706265794a7759584a68625756305a5849694f6e7369646d4673645755694f6e73695a47463059534936496d45354d44553559324a694d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774f57517a4e4745334d6a55325a6a6b784e4468684d546b79596d49304d3251345a6a51315a6d45794e6d55334d4756694d6d51794d7a41774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4449334d5441694c434a766432356c636c39685a4752795a584e7a496a6f694e444668596a566a4e575a694d545a6859544e6d5957566c596a413059546b344d6a49334d6d4a6d5a6a557a4e474d324d6d49344f475a6c496977695932397564484a68593352665957526b636d567a63794936496a5178595459784e4759344d444e694e6d5a6b4e7a67774f54673259545179597a63345a574d35597a646d4e7a646c4e6d526c5a44457a59794a394c434a306558426c5833567962434936496e5235634755755a3239765a32786c595842706379356a6232307663484a766447396a6232777556484a705a32646c636c4e7459584a305132397564484a68593351696653776964486c775a534936496c52796157646e5a584a546257467964454e76626e527959574e30496e31644c434a795a575a66596d787659327466596e6c305a584d694f6949324d44497849697769636d566d58324a7362324e7258326868633267694f694a6c4f4467335a575a6b4e7a6b79596a4a6d4d6a4269496977695a58687761584a6864476c76626949364d54637a4d6a4d7a4d7a51774e7a41774d4377695a6d566c5832787062576c30496a6f784e6a59354e4449794d43776964476c745a584e3059573177496a6f784e7a4d794d6a51324f5451354e6a49306661594241414141414141414d4745774d6a59774d6a45794d6a41345a5467344e32566d5a4463354d6d49795a6a4977596a51774f5467354e6d5934596a68694e544d794e5746685a5441784d4467785a6a457959546b774d5442684d7a45334e4463354e7a41324e544a6c4e6a63325a6a5a6d4e6a6332597a59314e6a45334d4459354e7a4d795a54597a4e6d59325a444a6d4e7a41334d6a5a6d4e7a51325a6a597a4e6d5932597a4a6c4e5451334d6a59354e6a63324e7a59314e7a49314d7a5a6b4e6a45334d6a63304e444d325a6a5a6c4e7a51334d6a59784e6a4d334e4445794e7a5177595445314e444668596a566a4e575a694d545a6859544e6d5957566c596a413059546b344d6a49334d6d4a6d5a6a557a4e474d324d6d49344f475a6c4d5449784e545178595459784e4759344d444e694e6d5a6b4e7a67774f54673259545179597a63345a574d35597a646d4e7a646c4e6d526c5a44457a597a49794e4452684f5441314f574e69596a41774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d446c6b4d7a52684e7a49314e6d59354d545134595445354d6d4a694e444e6b4f4759304e575a684d6a5a6c4e7a426c596a4a6b4d6a4d774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441774d4441794e7a45774e7a426d4f446c6b5a4749345a6d49314d7a49354d44417859324e6d4e325a684d446341414141414141414141413d3d01000000000000000000000000000000001400000000000000323032342d31312d32325430333a34323a32395a001200000000000000313838373538343630383939333937363332010000000000000030000000000000001200000000000000313938343932323839313030383132323838220000000000000054514a53415a6a3454357139424862513148677750484d7264385048683831765165820000000000000031353831393035653361373033353566376630346436366565303766313162616635333066633431653331653334326137316434336262376336336230623037366234383836333866376561613232636337336531646634636165393965626535326138383336333930653439356634373131356630323635636132363630383031011400000000000000323032342d31312d32325430393a35363a32305a00";

    let rs = MultisigQueueData::from_string(&str).unwrap();
    println!("{:#?}", rs);
}
