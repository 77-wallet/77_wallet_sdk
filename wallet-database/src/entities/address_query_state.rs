use serde::{Deserialize, Serialize};
use sqlx::types::chrono::Utc;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AddressQueryStateEntity {
    pub uid: String,
    pub chain_code: String,
    pub status: AddressQueryStatus,
    #[serde(skip_serializing)]
    pub created_at: chrono::DateTime<Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreateAddressQueryStateEntity {
    pub uid: String,
    pub chain_code: String,
    pub status: AddressQueryStatus,
}

impl CreateAddressQueryStateEntity {
    pub fn new(uid: &str, chain_code: &str, status: AddressQueryStatus) -> Self {
        Self { uid: uid.to_string(), chain_code: chain_code.to_string(), status }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    sqlx::Type,
)]
#[repr(u8)]
pub enum AddressQueryStatus {
    Running = 0,
    Done = 1,
    Failed = 2,
}
