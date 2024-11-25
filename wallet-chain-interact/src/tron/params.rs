use super::consts::TRX_TO_SUN;

#[derive(serde::Serialize, Debug)]
pub struct FreezeBalanceArgs {
    owner_address: String,
    resource: ResourceType,
    frozen_balance: i64,
    visible: bool,
}

impl FreezeBalanceArgs {
    pub fn new(owner_address: &str, resource: &str, frozen_balance: &str) -> crate::Result<Self> {
        let frozen_balance = wallet_utils::unit::convert_to_u256(frozen_balance, 6)?;

        Ok(Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(owner_address)?,
            resource: ResourceType::try_from(resource)?,
            frozen_balance: frozen_balance.to::<i64>(),
            visible: false,
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct UnFreezeBalanceArgs {
    owner_address: String,
    resource: ResourceType,
    unfreeze_balance: i64,
}

impl UnFreezeBalanceArgs {
    pub fn new(owner_address: &str, resource: &str, unfreeze_balance: &str) -> crate::Result<Self> {
        let unfreeze_balance = wallet_utils::unit::convert_to_u256(unfreeze_balance, 6)?;
        Ok(Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(owner_address)?,
            resource: ResourceType::try_from(resource)?,
            unfreeze_balance: unfreeze_balance.to::<i64>(),
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct DelegateArgs {
    pub owner_address: String,
    pub receiver_address: String,
    pub balance: u64,
    pub resource: ResourceType,
    pub lock: bool,
    pub lock_period: i64,
}
impl DelegateArgs {
    pub fn new(
        owner_address: &str,
        receiver_address: &str,
        balance: &str,
        resource: &str,
    ) -> crate::Result<Self> {
        let balance = wallet_utils::unit::convert_to_u256(balance, 6)?;
        Ok(Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(owner_address)?,
            receiver_address: wallet_utils::address::bs58_addr_to_hex(receiver_address)?,
            balance: balance.to::<u64>(),
            resource: ResourceType::try_from(resource)?,
            lock: false,
            lock_period: 0,
        })
    }

    pub fn with_lock_period(mut self, lock_period: i64) -> Self {
        self.lock = true;
        self.lock_period = lock_period;
        self
    }
}
#[derive(Debug, serde::Serialize)]
pub struct UnDelegateArgs {
    pub owner_address: String,
    pub receiver_address: String,
    pub balance: u64,
    pub resource: ResourceType,
}
impl UnDelegateArgs {
    pub fn new(
        owner_address: &str,
        receiver_address: &str,
        balance: &str,
        resource: &str,
    ) -> crate::Result<Self> {
        let balance = wallet_utils::unit::convert_to_u256(balance, 6)?;

        Ok(Self {
            owner_address: wallet_utils::address::bs58_addr_to_hex(owner_address)?,
            receiver_address: wallet_utils::address::bs58_addr_to_hex(receiver_address)?,
            balance: balance.to::<u64>(),
            resource: ResourceType::try_from(resource)?,
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub enum ResourceType {
    ENERGY,
    BANDWIDTH,
}

impl ResourceType {
    pub fn to_int_str(&self) -> String {
        match self {
            ResourceType::ENERGY => "1".to_string(),
            ResourceType::BANDWIDTH => "0".to_string(),
        }
    }
}
impl TryFrom<&str> for ResourceType {
    type Error = crate::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_ref() {
            "energy" => Ok(ResourceType::ENERGY),
            "bandwidth" => Ok(ResourceType::BANDWIDTH),
            _ => panic!("invalid resource type {:?}", value),
        }
    }
}

#[derive(Debug)]
pub struct Resource {
    // user resource
    pub limit: i64,
    // tx consumer resource
    pub consumer: i64,
    // unit is sun
    pub price: i64,
    // energy or bandwidth
    pub types: String,
}
impl Resource {
    pub fn new(limit: i64, consumer: i64, price: i64, types: &str) -> Self {
        Self {
            limit,
            consumer,
            price,
            types: types.to_string(),
        }
    }

    // 对于带宽来说，用户的免费带宽是小于交易所需要的带宽，那么这种情况，直接燃烧交易需要的带宽对应的trx
    pub fn need_extra_resource(&self) -> i64 {
        if self.types == "bandwidth" {
            if self.consumer > self.limit {
                self.consumer
            } else {
                0
            }
        } else if self.consumer > self.limit {
            self.consumer - self.limit
        } else {
            0
        }
    }

    pub fn fee(&self) -> i64 {
        self.price * self.need_extra_resource()
    }
}

#[derive(Debug)]
pub struct ResourceConsumer {
    pub energy: Option<Resource>,
    pub bandwidth: Resource,
    // unit is sun
    pub extra_fee: i64,
}

impl ResourceConsumer {
    pub fn new(bandwidth: Resource, energy: Option<Resource>) -> Self {
        Self {
            energy,
            bandwidth,
            extra_fee: 0,
        }
    }

    // unit is sun
    pub fn set_extra_fee(&mut self, extra_fee: i64) {
        self.extra_fee += extra_fee;
    }

    pub fn transaction_fee(&self) -> String {
        let bandwidth_fee = self.bandwidth.fee();
        let energy_fee = if let Some(energy) = self.energy.as_ref() {
            energy.fee()
        } else {
            0
        };

        let total = bandwidth_fee + energy_fee + self.extra_fee;

        (total as f64 / TRX_TO_SUN as f64).to_string()
    }

    // unit is sun
    pub fn transaction_fee_i64(&self) -> i64 {
        let bandwidth_fee = self.bandwidth.fee();
        let energy_fee = if let Some(energy) = self.energy.as_ref() {
            energy.fee()
        } else {
            0
        };

        bandwidth_fee + energy_fee + self.extra_fee
    }

    // 用于合约交易时设置fee_limit(不考虑用户用户的资源)
    pub fn fee_limit(&self) -> i64 {
        let mut fee = self.bandwidth.consumer * self.bandwidth.price;

        if let Some(energy) = &self.energy {
            fee += energy.consumer * energy.price;
        }

        fee
    }

    pub fn get_energy(&self) -> u64 {
        if let Some(energy) = &self.energy {
            energy.consumer as u64
        } else {
            0
        }
    }

    pub fn need_extra_energy(&self) -> i64 {
        if let Some(energy) = self.energy.as_ref() {
            energy.need_extra_resource()
        } else {
            0
        }
    }

    pub fn need_extra_bandwidth(&self) -> i64 {
        self.bandwidth.need_extra_resource()
    }
}
