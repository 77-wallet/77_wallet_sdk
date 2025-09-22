use crate::entities::chain::{ChainCreateVo, StringList};

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainEntity {
    pub id: i64,
    pub name: String,
    pub chain_code: String,
    pub main_symbol: String,
    pub node_id: Option<String>,
    // #[sqlx(type_name = "TEXT")]
    pub protocols: StringList,
    pub status: u8,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainWithNode {
    pub name: String,
    pub chain_code: String,
    pub main_symbol: String,
    pub node_id: String,
    pub node_name: String,
    pub rpc_url: String,
    pub ws_url: String,
    pub http_url: String,
    pub network: String,
    pub status: u8,
    #[serde(skip_serializing)]
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
impl ApiChainWithNode {
    pub fn get_network(&self) -> &str {
        if self.network.is_empty() { "mainnet" } else { &self.network }
    }
}

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
pub struct ApiChainCreateVo {
    pub name: String,
    pub chain_code: String,
    pub protocols: Vec<String>,
    pub status: u8,
    pub main_symbol: String,
}

impl ApiChainCreateVo {
    pub fn new(
        name: &str,
        chain_code: &str,
        protocols: &[String],
        main_symbol: &str,
    ) -> ApiChainCreateVo {
        Self {
            name: name.to_string(),
            chain_code: chain_code.to_string(),
            protocols: protocols.to_vec(),
            status: 1,
            main_symbol: main_symbol.to_string(),
        }
    }

    pub fn with_status(mut self, status: u8) -> Self {
        self.status = status;
        self
    }
}