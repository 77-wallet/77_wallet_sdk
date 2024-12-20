#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateOrderArgs {
    pub address: String,
    pub energy_amount: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateQueryResp {
    #[serde(default)]
    pub order_id: String,
    pub energy_hash: Option<String>,
    pub energy_status: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEnergyResp {
    // 用户单日能量补贴总次数
    pub time_limit: i64,
    // 用户单日剩余总次数
    pub left_times: i64,
    // 当笔交易最大补贴能量
    pub energy_limit: i64,
    // 当前可用能量)
    pub left_energy: i64,
    // 领取状态
    pub status: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListWitnessResp {
    pub witnesses: Vec<Witness>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Witness {
    pub address: String,
    pub vote_count: Option<i64>,
    pub url: String,
    pub brokerage: i64,
}

impl Witness {
    pub fn new(address: &str, vote_count: Option<i64>, url: &str, brokerage: i64) -> Self {
        Self {
            address: wallet_utils::address::bs58_addr_to_hex(address).unwrap(),
            vote_count,
            url: url.to_string(),
            brokerage,
        }
    }
}
