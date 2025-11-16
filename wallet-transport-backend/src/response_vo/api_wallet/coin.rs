#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiCoinInfo {
    pub id: String,
    #[serde(
        rename = "code",
        deserialize_with = "wallet_utils::serde_func::deserialize_uppercase_opt"
    )]
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub chain_code: Option<String>,
    #[serde(rename = "contractAddress")]
    pub token_address: Option<String>,
    pub protocol: Option<String>,
    #[serde(rename = "unit")]
    pub decimals: Option<u8>,
    #[serde(default, deserialize_with = "wallet_utils::serde_func::deserialize_default_false")]
    pub enable: bool,
    pub price: Option<f64>,
    // 币是否支持兑换
    #[serde(default, deserialize_with = "wallet_utils::serde_func::deserialize_default_false")]
    pub swappable: bool,
    #[serde(default, deserialize_with = "wallet_utils::serde_func::deserialize_default_false")]
    pub default_token: bool,
    #[serde(default, deserialize_with = "wallet_utils::serde_func::deserialize_default_false")]
    pub popular_token: bool,
    pub create_time: String,
    pub update_time: String,
}
impl ApiCoinInfo {
    pub fn get_status(&self) -> Option<i32> {
        if self.enable { Some(1) } else { Some(0) }
    }
}
