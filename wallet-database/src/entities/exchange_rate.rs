// #[derive(Debug, PartialEq, Eq, Hash)]
// pub struct ExchangeRateId {
//     pub currency: String,
//     pub chain_code: String,
//     pub symbol: String,
// }

// impl ExchangeRateId {
//     pub fn new(currency: &str, chain_code: &str, symbol: &str) -> Self {
//         Self {
//             currency: currency.to_string(),
//             chain_code: chain_code.to_string(),
//             symbol: symbol.to_string(),
//         }
//     }
// }

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRateEntity {
    pub name: String,
    pub rate: f64,
    pub target_currency: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(Debug, PartialEq)]
pub struct ExchangeRate {
    pub name: String,
    pub price: f64,
}
