use strum_macros::Display;

#[derive(
    sqlx::Type,
    Debug,
    Clone,
    Copy,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
    PartialEq,
    Eq,
    PartialOrd,
    Display,
)]
#[repr(u8)]
pub enum ApiTradeType {
    Withdraw = 1,
    Collect = 2,
    TransferFee = 3,
    SelfWithdraw = 4,
    SelfRecharge = 5,
}
