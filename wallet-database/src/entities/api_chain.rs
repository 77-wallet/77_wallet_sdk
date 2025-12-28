use crate::entities::chain::{ChainWithNode, StringList};

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainEntity {
    #[serde(skip_serializing)]
    pub id: i64,
    pub name: String,
    pub chain_code: String,
    pub main_symbol: String,
    pub node_id: Option<String>,
    // #[sqlx(type_name = "TEXT")]
    pub protocols: StringList,
    pub node_bind_type: NodeBindType,
    pub status: u8,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}

#[derive(
    Debug, Clone, serde_repr::Serialize_repr, serde_repr::Deserialize_repr, sqlx::Type, PartialEq,
)]
#[repr(u8)]
pub enum NodeBindType {
    AutoBackend = 0,
    AutoLocal = 1,
    ManualUser = 2,
}

pub(crate) type ApiChainWithNode = ChainWithNode;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct ApiChainCreateVo {
    pub name: String,
    pub chain_code: String,
    pub protocols: Vec<String>,
    pub node_bind_type: NodeBindType,
    pub status: u8,
    pub main_symbol: String,
}

impl ApiChainCreateVo {
    pub fn new(
        name: &str,
        chain_code: &str,
        protocols: &[String],
        node_bind_type: NodeBindType,
        main_symbol: &str,
    ) -> ApiChainCreateVo {
        Self {
            name: name.to_string(),
            chain_code: chain_code.to_string(),
            protocols: protocols.to_vec(),
            node_bind_type,
            status: 1,
            main_symbol: main_symbol.to_string(),
        }
    }

    pub fn with_status(mut self, status: u8) -> Self {
        self.status = status;
        self
    }
}

#[async_trait::async_trait]
impl super::chain::ChainLike for ApiChainEntity {
    fn chain_code(&self) -> &str {
        &self.chain_code
    }
    fn status(&self) -> u8 {
        self.status
    }
    fn node_id(&self) -> Option<&String> {
        self.node_id.as_ref()
    }

    async fn set_node(
        pool: &crate::DbPool,
        chain_code: &str,
        node_id: &str,
        bind_type: NodeBindType,
    ) -> Result<(), crate::Error> {
        use crate::repositories::api_wallet::chain::ApiChainRepo;
        ApiChainRepo::set_chain_node_with_type(pool, chain_code, node_id, bind_type).await?;
        Ok(())
    }
}
