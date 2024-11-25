use sqlx::types::chrono::{DateTime, Utc};

pub mod config_key {
    pub const MIN_VALUE_SWITCH: &str = "min_value_switch";
}

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntity {
    pub id: u32,
    pub key: String,
    pub value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct MinValueSwitchConfig {
    // true 开启状态 false 关闭状态
    pub switch: bool,
    // 配置的金额
    pub value: f64,
    // 金额对应的法币符号
    pub currency: String,
}
impl MinValueSwitchConfig {
    pub fn to_json_str(&self) -> Result<String, crate::Error> {
        Ok(wallet_utils::serde_func::serde_to_string(self)?)
    }
}

impl TryFrom<String> for MinValueSwitchConfig {
    type Error = crate::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ok(wallet_utils::serde_func::serde_from_str(&value)?)
    }
}
