#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    sqlx::Type,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
)]
#[repr(i64)]
pub enum ApiResourceGateResult {
    ResourceReady = 1,
    ResourceDelegationSuccess = 2,
    ResourceDelegationFailedBypass = 3,
    LocalDelegationSuccess = 4,
    LocalDelegationFailedBypass = 5,
    PlatformDelegateSuccess = 6,
}

impl ApiResourceGateResult {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    sqlx::Type,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
)]
#[repr(i64)]
pub enum ApiResourceBlockReason {
    NeedPlatformDelegate = 1,
    NeedLocalDelegate = 2,
}

impl ApiResourceBlockReason {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    sqlx::Type,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
)]
#[repr(i64)]
pub enum ApiResourceDependencyType {
    PlatformDelegate = 1,
    LocalDelegate = 2,
}

impl ApiResourceDependencyType {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}
