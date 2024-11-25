use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tron::consts;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default)]
pub struct TronAccount {
    pub address: String,
    // unit is trx
    pub balance: i64,
    pub account_resource: AccountResource,
    #[serde(rename = "delegated_frozenV2_balance_for_bandwidth")]
    pub delegated_bandwidth: i64,
    #[serde(rename = "acquired_delegated_frozenV2_balance_for_bandwidth")]
    pub acquired_bandwidth: i64,
    #[serde(rename = "frozenV2")]
    pub frozen_v2: Vec<FrozenV2>,
    #[serde(rename = "unfrozenV2")]
    pub unfreeze_v2: Vec<UnfrozenV2>,
    pub owner_permission: PermissionResp,
    pub active_permission: Vec<PermissionResp>,
    #[serde(flatten)]
    #[serde(default)]
    extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

impl TronAccount {
    pub fn frozen_v2_owner(&self, resource_type: &str) -> i64 {
        self.frozen_v2
            .iter()
            .filter(|item| item.types == resource_type)
            .map(|item| item.amount)
            .sum::<i64>()
    }

    pub fn can_withdraw_unfreeze_amount(&self, resource_type: &str) -> i64 {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let timestamp = since_the_epoch.as_micros() as i64;

        self.unfreeze_v2
            .iter()
            .filter(|item| item.types == resource_type && item.unfreeze_expire_time <= timestamp)
            .map(|item| item.unfreeze_amount)
            .sum::<i64>()
    }

    pub fn balance_to_f64(&self) -> f64 {
        self.balance as f64 / consts::TRX_TO_SUN as f64
    }

    pub fn is_multisig_account(&self) -> bool {
        self.active_permission
            .iter()
            .any(|permission| permission.keys.len() >= 2)
            || self.owner_permission.keys.len() >= 2
    }
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default)]
pub struct FrozenV2 {
    pub amount: i64,
    #[serde(rename = "type")]
    pub types: String,
    #[serde(flatten)]
    #[serde(default)]
    extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default)]
pub struct UnfrozenV2 {
    #[serde(rename = "type")]
    pub types: String,
    pub unfreeze_amount: i64,
    pub unfreeze_expire_time: i64,
    #[serde(flatten)]
    #[serde(default)]
    extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default)]
pub struct AccountResource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_consume_time_for_energy: Option<u64>,
    pub energy_window_size: u64,
    pub energy_window_optimized: bool,
    #[serde(default, rename = "delegated_frozenV2_balance_for_energy")]
    pub delegated_energy: i64,
    #[serde(default, rename = "acquired_delegated_frozenV2_balance_for_energy")]
    pub acquired_energy: i64,
    #[serde(flatten)]
    #[serde(default)]
    extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default, rename_all = "PascalCase")]
pub struct AccountResourceDetail {
    #[serde(rename = "freeNetUsed")]
    pub free_net_used: i64,
    #[serde(rename = "freeNetLimit")]
    pub free_net_limit: i64,
    pub net_used: i64,
    pub net_limit: i64,
    pub total_net_limit: i64,
    pub total_net_weight: i64,
    pub energy_used: i64,
    pub energy_limit: i64,
    pub total_energy_limit: i64,
    pub total_energy_weight: i64,
}
impl AccountResourceDetail {
    pub fn energy_price(&self) -> f64 {
        if self.total_energy_weight == 0 {
            return 0.0;
        }
        self.total_energy_limit as f64 / self.total_energy_weight as f64
    }
    pub fn net_price(&self) -> f64 {
        if self.total_net_weight == 0 {
            return 0.0;
        }
        self.total_net_limit as f64 / self.total_net_weight as f64
    }

    pub fn available_bandwidth(&self) -> i64 {
        ((self.net_limit + self.free_net_limit) - (self.net_used + self.free_net_used)).max(0)
    }

    pub fn available_stake_bandwidth(&self) -> i64 {
        (self.net_limit - self.net_used).max(0)
    }

    pub fn available_energy(&self) -> i64 {
        (self.energy_limit - self.energy_used).max(0)
    }
}

/// multi sig account permission
#[derive(Debug, Serialize, Deserialize)]
pub struct AccountPermission<T> {
    owner_address: String,
    owner: T,
    actives: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Permission {
    #[serde(rename = "type")]
    pub types: i8,
    pub permission_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operations: Option<String>,
    pub threshold: u8,
    pub keys: Vec<Keys>,
}
#[derive(Serialize, Debug, Deserialize, Default)]
pub struct PermissionResp {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,
    pub permission_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operations: Option<String>,
    pub threshold: u8,
    pub keys: Vec<Keys>,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Keys {
    address: String,
    weight: i8,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct FreezeBalanceResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<String>,
    frozen_balance: i64,
    owner_address: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct UnFreezeBalanceResp {
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<String>,
    unfreeze_balance: i64,
    owner_address: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct DelegateResp {
    owner_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<String>,
    receiver_address: String,
    balance: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lock_period: Option<i64>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct UnDelegateResp {
    owner_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<String>,
    receiver_address: String,
    balance: i64,
}

#[derive(serde::Deserialize)]
pub struct CanWithdrawUnfreezeAmount {
    #[serde(default)]
    pub amount: i64,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct WithdrawExpire {
    owner_address: String,
}
