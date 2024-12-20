use wallet_chain_interact::tron::{
    consts,
    operations::stake::{self, DelegateArgs, UnDelegateArgs, UnFreezeBalanceArgs},
};

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct FreezeBalanceReq {
    pub owner_address: String,
    pub resource: String,
    pub frozen_balance: i64,
}
impl TryFrom<&FreezeBalanceReq> for stake::FreezeBalanceArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &FreezeBalanceReq) -> Result<Self, Self::Error> {
        let args = stake::FreezeBalanceArgs::new(
            &value.owner_address,
            &value.resource,
            value.frozen_balance,
        )?;
        Ok(args)
    }
}

#[derive(serde::Serialize, Debug)]
pub struct UnFreezeBalanceReq {
    pub owner_address: String,
    pub resource: String,
    pub unfreeze_balance: i64,
}

impl From<&UnFreezeBalanceReq> for wallet_database::entities::stake::NewUnFreezeEntity {
    fn from(value: &UnFreezeBalanceReq) -> Self {
        Self {
            tx_hash: "".to_string(),
            owner_address: value.owner_address.clone(),
            resource_type: value.resource.clone(),
            amount: value.unfreeze_balance.to_string(),
            freeze_time: 0,
        }
    }
}

impl TryFrom<&UnFreezeBalanceReq> for UnFreezeBalanceArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &UnFreezeBalanceReq) -> Result<Self, Self::Error> {
        let args = stake::UnFreezeBalanceArgs::new(
            &value.owner_address,
            &value.resource,
            value.unfreeze_balance,
        )?;
        Ok(args)
    }
}

#[derive(serde::Serialize, Debug)]
pub struct DelegateReq {
    pub owner_address: String,
    pub receiver_address: String,
    pub balance: i64,
    pub resource: String,
    pub lock: bool,
    pub lock_period: i64,
}

impl From<&DelegateReq> for wallet_database::entities::stake::NewDelegateEntity {
    fn from(value: &DelegateReq) -> Self {
        Self {
            tx_hash: "".to_string(),
            owner_address: value.owner_address.clone(),
            receiver_address: value.receiver_address.clone(),
            amount: value.balance.to_string(),
            resource_type: value.resource.clone(),
            lock: value.lock.into(),
            lock_period: value.lock_period,
        }
    }
}

impl TryFrom<&DelegateReq> for DelegateArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &DelegateReq) -> Result<Self, Self::Error> {
        let args = Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(&value.owner_address)?,
            receiver_address: wallet_utils::address::bs58_addr_to_hex(&value.receiver_address)?,
            balance: value.balance * consts::TRX_VALUE,
            resource: stake::ResourceType::try_from(value.resource.as_str())?,
            lock: value.lock,
            lock_period: value.lock_period,
        };
        Ok(args)
    }
}

#[derive(serde::Serialize, Debug)]
pub struct UnDelegateReq {
    pub owner_address: String,
    pub resource: String,
    pub receiver_address: String,
    pub balance: i64,
}

impl TryFrom<&UnDelegateReq> for UnDelegateArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &UnDelegateReq) -> Result<Self, Self::Error> {
        let args = Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(&value.owner_address)?,
            receiver_address: wallet_utils::address::bs58_addr_to_hex(&value.receiver_address)?,
            balance: value.balance * consts::TRX_VALUE,
            resource: stake::ResourceType::try_from(value.resource.as_str())?,
        };
        Ok(args)
    }
}
