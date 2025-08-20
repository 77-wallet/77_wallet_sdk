use crate::entities::assets::AssetsId;

#[derive(Debug, Default, serde::Serialize, sqlx::FromRow)]
pub struct ApiAssetsEntity {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub address: String,
    pub chain_code: String,
    pub token_address: String,
    pub protocol: Option<String>,
    pub status: u8,
    /// 0/普通资产 1/多签资产 2/待部署多签账户的普通资产
    pub is_multisig: i8,
    pub balance: String,
    #[serde(skip_serializing)]
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    #[serde(skip_serializing)]
    pub updated_at: Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,
}
impl ApiAssetsEntity {
    pub fn get_assets_id(&self) -> AssetsId {
        AssetsId {
            address: self.address.clone(),
            symbol: self.symbol.clone(),
            chain_code: self.chain_code.clone(),
            token_address: self.token_address(),
        }
    }

    pub fn token_address(&self) -> Option<String> {
        if self.token_address.is_empty() {
            None
        } else {
            Some(self.token_address.clone())
        }
    }
}

#[derive(Debug)]
pub struct ApiCreateAssetsVo {
    pub assets_id: AssetsId,
    pub name: String,
    pub decimals: u8,
    pub protocol: Option<String>,
    pub status: u8,
    pub is_multisig: i32,
    pub balance: String,
}

impl ApiCreateAssetsVo {
    pub fn new(
        assets_id: AssetsId,
        decimals: u8,
        protocol: Option<String>,
        is_multisig: i32,
    ) -> Self {
        Self {
            assets_id,
            name: "name".to_string(),
            decimals,
            protocol,
            status: 1,
            is_multisig,
            balance: "0.00".to_string(),
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_status(mut self, status: u8) -> Self {
        self.status = status;
        self
    }

    pub fn with_u256(
        mut self,
        balance: alloy::primitives::U256,
        decimals: u8,
    ) -> Result<Self, crate::Error> {
        let balance = wallet_utils::unit::format_to_string(balance, decimals)?;
        let balance = wallet_utils::parse_func::decimal_from_str(&balance)?;

        self.balance = balance.to_string();
        Ok(self)
    }

    pub fn with_balance(mut self, balance: &str) -> Self {
        self.balance = balance.to_string();
        self
    }

    pub fn with_protocol(mut self, protocol: Option<String>) -> Self {
        self.protocol = protocol;
        self
    }
}
