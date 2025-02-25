use wallet_chain_interact::tron::operations::permisions::{ContractType, PermissionTypes};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionList {
    // 资产权限
    pub assets: Vec<i8>,
    // 资源
    pub resource: Vec<i8>,
    // 账号权限
    pub accounts: Vec<i8>,
    // TRC1.0
    pub trx_assets: Vec<i8>,
    // 智能合约
    pub contract: Vec<i8>,
    // 超级代表
    pub super_present: Vec<i8>,
    // bancor
    pub bancor: Vec<i8>,
}

impl Default for PermissionList {
    fn default() -> Self {
        Self {
            assets: vec![
                ContractType::TransferContract.to_i8(),
                ContractType::FreezeBalanceContract.to_i8(),
                ContractType::UnfreezeBalanceContract.to_i8(),
                ContractType::FreezeBalanceV2Contract.to_i8(),
                ContractType::UnfreezeBalanceV2Contract.to_i8(),
                ContractType::WithdrawExpireUnfreezeContract.to_i8(),
            ],
            resource: vec![
                ContractType::DelegateResourceContract.to_i8(),
                ContractType::UnDelegateResourceContract.to_i8(),
            ],
            accounts: vec![
                ContractType::AccountCreateContract.to_i8(),
                ContractType::AccountUpdateContract.to_i8(),
            ],
            trx_assets: vec![
                ContractType::TransferAssetContract.to_i8(),
                ContractType::AssetIssueContract.to_i8(),
                ContractType::UpdateAssetContract.to_i8(),
            ],
            contract: vec![
                ContractType::CreateSmartContract.to_i8(),
                ContractType::TriggerSmartContract.to_i8(),
                ContractType::UpdateSettingContract.to_i8(),
                ContractType::UpdateEnergyLimitContract.to_i8(),
                ContractType::ClearABIContract.to_i8(),
            ],
            super_present: vec![
                ContractType::VoteWitnessContract.to_i8(),
                ContractType::WithdrawBalanceContract.to_i8(),
                ContractType::ProposalCreateContract.to_i8(),
                ContractType::ProposalApproveContract.to_i8(),
                ContractType::ProposalDeleteContract.to_i8(),
                ContractType::WitnessCreateContract.to_i8(),
                ContractType::WitnessUpdateContract.to_i8(),
                ContractType::UpdateBrokerageContract.to_i8(),
            ],
            bancor: vec![
                ContractType::ExchangeCreateContract.to_i8(),
                ContractType::ExchangeInjectContract.to_i8(),
                ContractType::ExchangeWithdrawContract.to_i8(),
                ContractType::ExchangeTransactionContract.to_i8(),
            ],
        }
    }
}

impl PermissionList {
    // 与交易相关的权限，在交易里面使用
    pub fn trans_permission() -> Vec<i8> {
        vec![
            ContractType::TransferContract.to_i8(),
            ContractType::FreezeBalanceV2Contract.to_i8(),
            ContractType::UnfreezeBalanceV2Contract.to_i8(),
            ContractType::CancelAllUnfreezeV2Contract.to_i8(),
            ContractType::WithdrawExpireUnfreezeContract.to_i8(),
            ContractType::DelegateResourceContract.to_i8(),
            ContractType::UnDelegateResourceContract.to_i8(),
            ContractType::VoteWitnessContract.to_i8(),
            ContractType::WithdrawBalanceContract.to_i8(),
        ]
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPermission {
    pub owner: PermissionResp,
    pub actives: Vec<PermissionResp>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionResp {
    pub name: String,
    pub threshold: i8,
    pub operations: Option<Vec<i8>>,
    pub keys: Vec<Keys>,
}

impl TryFrom<&wallet_chain_interact::tron::protocol::account::PermissionResp> for PermissionResp {
    type Error = crate::ServiceError;

    fn try_from(
        value: &wallet_chain_interact::tron::protocol::account::PermissionResp,
    ) -> Result<Self, Self::Error> {
        let operations = if value.permission_name == "owner" {
            None
        } else {
            let operations = PermissionTypes::from_hex(&value.operations.as_ref().unwrap())?;
            Some(operations)
        };

        Ok(PermissionResp {
            name: value.permission_name.clone(),
            threshold: value.threshold as i8,
            operations,
            keys: value.keys.iter().map(|k| Keys::from(k)).collect(),
        })
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    address: String,
    weight: i8,
}

impl From<&wallet_chain_interact::tron::protocol::account::Keys> for Keys {
    fn from(value: &wallet_chain_interact::tron::protocol::account::Keys) -> Self {
        Keys {
            address: value.address.clone(),
            weight: value.weight.clone(),
        }
    }
}
