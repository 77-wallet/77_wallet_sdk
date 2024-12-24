use wallet_chain_interact::tron::{
    consts,
    operations::stake::{
        self, DelegateArgs, UnDelegateArgs, UnFreezeBalanceArgs, VoteWitnessArgs,
        WithdrawBalanceArgs,
    },
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

#[derive(serde::Serialize, Debug, serde::Deserialize)]
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

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct CancelAllUnFreezeReq {
    pub owner_address: String,
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
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

#[derive(serde::Serialize, Debug, serde::Deserialize)]
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

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct VoteWitnessReq {
    pub owner_address: String,
    pub votes: Vec<VotesReq>,
}

impl VoteWitnessReq {
    pub fn new(owner_address: &str, votes: Vec<VotesReq>) -> Self {
        Self {
            owner_address: owner_address.to_string(),
            votes,
        }
    }
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct VotesReq {
    pub vote_address: String,
    pub vote_count: i64,
}

impl VotesReq {
    pub fn new(vote_address: &str, vote_count: i64) -> Self {
        Self {
            vote_address: vote_address.to_string(),
            vote_count,
        }
    }
}

impl TryFrom<&VoteWitnessReq> for VoteWitnessArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &VoteWitnessReq) -> Result<Self, Self::Error> {
        let mut votes = Vec::new();
        for v in &value.votes {
            let vote = stake::Votes::new(&v.vote_address, v.vote_count)?;
            votes.push(vote);
        }

        Ok(VoteWitnessArgs::new(&value.owner_address, votes)?)
    }
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct WithdrawBalanceReq {
    pub owner_address: String,
}

impl WithdrawBalanceReq {
    pub fn new(owner_address: &str) -> Self {
        Self {
            owner_address: owner_address.to_string(),
        }
    }
}

impl TryFrom<&WithdrawBalanceReq> for WithdrawBalanceArgs {
    type Error = crate::error::ServiceError;
    fn try_from(value: &WithdrawBalanceReq) -> Result<Self, Self::Error> {
        let args = Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(&value.owner_address)?,
        };

        Ok(args)
    }
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
// 批量取消代理
pub struct BatchDelegate {
    pub owner_address: String,
    pub resource_type: String,
    pub lock: bool,
    pub lock_period: i64,
    pub list: Vec<BatchList>,
}

impl BatchDelegate {
    pub fn total(&self) -> i64 {
        self.list.iter().map(|t| t.value).sum()
    }
}

impl TryFrom<&BatchDelegate> for Vec<DelegateArgs> {
    type Error = crate::error::ServiceError;
    fn try_from(value: &BatchDelegate) -> Result<Self, Self::Error> {
        let owner_address = wallet_utils::address::bs58_addr_to_hex(&value.owner_address)?;
        let resource_type = stake::ResourceType::try_from(value.resource_type.as_str())?;

        value
            .list
            .iter()
            .map(|item| {
                Ok(DelegateArgs {
                    owner_address: owner_address.clone(),
                    receiver_address: wallet_utils::address::bs58_addr_to_hex(
                        &item.revevie_address,
                    )?,
                    balance: item.value * consts::TRX_VALUE,
                    resource: resource_type,
                    lock: value.lock,
                    lock_period: value.lock_period,
                })
            })
            .collect()
    }
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
// 批量取消代理
pub struct BatchUnDelegate {
    pub owner_address: String,
    pub resource_type: String,
    pub list: Vec<BatchList>,
}
impl BatchUnDelegate {
    pub fn total(&self) -> i64 {
        self.list.iter().map(|t| t.value).sum()
    }
}

impl TryFrom<&BatchUnDelegate> for Vec<UnDelegateArgs> {
    type Error = crate::error::ServiceError;
    fn try_from(value: &BatchUnDelegate) -> Result<Self, Self::Error> {
        let owner_address = wallet_utils::address::bs58_addr_to_hex(&value.owner_address)?;
        let resource_type = stake::ResourceType::try_from(value.resource_type.as_str())?;

        value
            .list
            .iter()
            .map(|item| {
                Ok(UnDelegateArgs {
                    owner_address: owner_address.clone(),
                    receiver_address: wallet_utils::address::bs58_addr_to_hex(
                        &item.revevie_address,
                    )?,
                    balance: item.value * consts::TRX_VALUE,
                    resource: resource_type,
                })
            })
            .collect()
    }
}

#[derive(serde::Serialize, Debug, serde::Deserialize)]
pub struct BatchList {
    pub revevie_address: String,
    pub value: i64,
}
