#[derive(
    sqlx::Type,
    Debug,
    Clone,
    Copy,
    serde_repr::Deserialize_repr,
    serde_repr::Serialize_repr,
    PartialEq,
    Eq,
)]
#[repr(u8)]
pub enum ApiWithdrawTradeType {
    Withdraw = 1,
    Collect = 2,
    TransferFee = 3,
    SelfWithdraw = 4,
    SelfRecharge = 5,
}
