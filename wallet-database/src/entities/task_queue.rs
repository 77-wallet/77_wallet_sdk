use std::str::FromStr as _;

use serde::{Deserialize, Serialize};

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct TaskQueueEntity {
    pub id: String,
    pub task_name: TaskName,
    pub request_body: String,
    pub r#type: u8,
    /// 0: pending, 1: running, 2: success, 3: failed, 4: hang up
    pub status: u8,
    // pub wallet_type: Option<WalletType>,
    #[serde(skip_serializing)]
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateTaskQueueEntity {
    pub id: String,
    pub task_name: TaskName,
    pub request_body: Option<String>,
    pub r#type: u8,
    pub status: u8,
    // pub wallet_type: Option<WalletType>,
}

impl CreateTaskQueueEntity {
    pub fn new(
        id: Option<String>,
        task_name: TaskName,
        request_body: Option<String>,
        r#type: u8,
        status: u8,
        // wallet_type: Option<WalletType>,
    ) -> Result<Self, crate::Error> {
        let id = id.unwrap_or_else(|| wallet_utils::snowflake::get_uid().unwrap().to_string());
        Ok(Self {
            id,
            task_name,
            request_body,
            r#type,
            status,
            // wallet_type,
        })
    }

    pub fn with_backend_request_string(
        task_name: TaskName,
        request_body: Option<String>,
        // wallet_type: Option<WalletType>,
    ) -> Result<Self, crate::Error> {
        Self::new(None, task_name, request_body, 1, 0)
    }

    pub fn with_mqtt_request_string(
        id: &str,
        task_name: TaskName,
        request_body: Option<String>,
        // wallet_type: Option<WalletType>,
    ) -> Result<Self, crate::Error> {
        Self::new(Some(id.to_string()), task_name, request_body, 2, 0)
    }

    pub fn with_backend_request<T: serde::Serialize>(
        task_name: TaskName,
        request_body: Option<&T>,
        // wallet_type: Option<WalletType>,
    ) -> Result<Self, crate::Error> {
        let request_body = request_body
            .map(wallet_utils::serde_func::serde_to_string)
            .transpose()?;
        Self::new(None, task_name, request_body, 1, 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskName {
    Known(KnownTaskName),
    Unknown(String),
}

impl sqlx::Type<sqlx::Sqlite> for TaskName {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}
impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for TaskName {
    // fn encode_by_ref(&self, buf: &mut sqlx::sqlite::SqliteArguments<'q>) -> IsNull {
    //     let s: &str = match self {
    //         TaskName::Known(k) => k.as_ref(),
    //         TaskName::Unknown(s) => s.as_str(),
    //     };
    //     buf.add(SqliteArgumentValue::Text(s.into()));
    //     IsNull::No
    // }
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::database::HasArguments<'q>>::ArgumentBuffer,
    ) -> sqlx::encode::IsNull {
        let s = match self {
            TaskName::Known(k) => k.as_ref().to_string(),
            TaskName::Unknown(s) => s.as_str().to_string(),
        };
        buf.push(sqlx::sqlite::SqliteArgumentValue::Text(s.into()));
        sqlx::encode::IsNull::No
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for TaskName {
    fn decode(
        value: <sqlx::Sqlite as sqlx::database::HasValueRef<'r>>::ValueRef,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let raw = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(match KnownTaskName::from_str(&raw) {
            Ok(known) => TaskName::Known(known),
            Err(_) => TaskName::Unknown(raw),
        })
    }
}

impl From<String> for TaskName {
    fn from(s: String) -> Self {
        match s.parse::<KnownTaskName>() {
            Ok(k) => TaskName::Known(k),
            Err(_) => TaskName::Unknown(s),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    sqlx::Type,
    PartialEq,
    Eq,
    Hash,
    strum_macros::EnumString,
    strum_macros::AsRefStr,
)]
#[sqlx(rename_all = "PascalCase")]
#[sqlx(type_name = "TEXT")]
pub enum KnownTaskName {
    PullAnnouncement,
    PullHotCoins,
    SetBlockBrowserUrl,
    SetFiat,
    RecoverQueueData,
    InitMqtt,
    BackendApi,
    // mqtt
    OrderMultiSignAccept,
    OrderMultiSignAcceptCompleteMsg,
    OrderMultiSignServiceComplete,
    OrderMultiSignCancel,
    MultiSignTransAccept,
    MultiSignTransCancel,
    MultiSignTransAcceptCompleteMsg,
    MultiSignTransExecute,
    AcctChange,
    OrderMultiSignCreated,
    BulletinMsg,
    // TronSignFreezeDelegateVoteChange,
    PermissionAccept,
    CleanPermission,
    // RpcChange,
    // common
    QueryCoinPrice,
    QueryQueueResult,
    RecoverMultisigAccountData,
    SyncNodesAndLinkToChains,
    // RecoverPermission,
    OrderAllConfirmed,
    UnbindUid,
    AddressUse,
    ApiWithdraw,
}
