use chrono::Utc;
use wallet_database::entities::api_bill::{ApiBillEntity, ApiBillKind};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiWithdrawReq {
    pub uid: String, // 钱包
    pub from: String,
    pub to: String,
    pub value: String,
    #[serde(rename = "chain")]
    pub chain_code: String,
    #[serde(rename = "token_addr")]
    pub token_address: Option<String>,
    #[serde(rename = "token_code")]
    pub symbol: String,
    pub trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    pub trade_type: u8,
}

#[derive(Debug, Clone)]
pub struct ApiBaseTransferReq {
    pub from: String,
    pub to: String,
    pub value: String,
    pub chain_code: String,
    pub token_address: Option<String>,
    pub decimals: u8,
    pub symbol: String,
    // 用户后端回收资源的id
    pub request_resource_id: Option<String>,
    // pub address_type: Option<String>,
    pub spend_all: bool,
    pub notes: Option<String>,
}

impl ApiBaseTransferReq {
    pub fn new(from: &str, to: &str, value: &str, chain_code: &str) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            value: value.to_string(),
            chain_code: chain_code.to_string(),
            token_address: None,
            decimals: 0,
            symbol: "".to_string(),
            request_resource_id: None,

            // address_type: None,
            spend_all: false,
            notes: None,
        }
    }

    pub fn with_token(&mut self, token_address: Option<String>, decimals: u8, symbol: &str) {
        self.token_address = token_address;
        self.decimals = decimals;
        self.symbol = symbol.to_string();
    }

    pub fn with_request_resource_id(&mut self, request_resource_id: Option<String>) {
        self.request_resource_id = request_resource_id
    }

    pub fn with_spend_all(&mut self, spend_all: bool) {
        self.spend_all = spend_all;
    }

    pub fn with_notes(&mut self, notes: String) {
        self.notes = Some(notes);
    }
}

#[derive(Debug, Clone)]
pub struct ApiTransferReq {
    pub base: ApiBaseTransferReq,
    pub password: String,
}

impl TryFrom<&ApiTransferReq> for ApiBillEntity {
    type Error = crate::ServiceError;

    fn try_from(req: &ApiTransferReq) -> Result<Self, Self::Error> {
        let value = wallet_utils::unit::string_to_f64(&req.base.value)?;
        let res = ApiBillEntity {
            id: 0,
            hash: "".to_string(),
            from_addr: req.base.from.clone(),
            to_addr: req.base.to.clone(),
            token: req.base.token_address.clone(),
            value: req.base.value.clone(),
            symbol: req.base.symbol.clone(),
            chain_code: req.base.chain_code.clone(),
            tx_kind: ApiBillKind::Transfer,
            owner: "".to_string(),
            status: 1,
            queue_id: "".to_owned(),
            notes: req.base.notes.clone().unwrap_or_default(),
            transaction_fee: "0".to_string(),
            resource_consume: "".to_string(),
            block_height: "0".to_string(),
            signer: "".to_string(),
            extra: "".to_string(),
            created_at: Default::default(),
            transfer_type: 0,
            is_multisig: 0,
            updated_at: None,
            transaction_time: Utc::now(),
        };
        Ok(res)
    }
}
