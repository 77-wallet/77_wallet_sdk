// biz_type = ENERGY_STAKE_CONFIRM
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergyStakeConfirmFrontend {}

// biz_type = ENERGY_STAKE_SUCCESS
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergyStakeSuccessFrontend {}

// biz_type = ENERGY_STAKE_FAILED
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnergyStakeFailedFrontend {}
