// 先放在这里，不知道最终会不会和后端用一个
use crate::request::transaction::SwapTokenListReq;
use alloy::primitives::U256;
use wallet_transport::client::HttpClient;

pub struct SwapClient {
    client: HttpClient,
}

impl SwapClient {
    pub fn new(url: &str) -> Result<Self, crate::ServiceError> {
        let client = HttpClient::new(&url, None, None)?;

        Ok(Self { client })
    }

    fn handle_result<T: serde::de::DeserializeOwned>(
        &self,
        res: AggregatorResp<T>,
    ) -> Result<T, crate::ServiceError> {
        if res.code == 200 {
            Ok(res.data)
        } else {
            Err(crate::ServiceError::AggregatorError(
                res.msg.unwrap_or_default(),
            ))
        }
    }

    pub async fn get_quote(
        &self,
        req: AggQuoteRequest,
    ) -> Result<AggQuoteResp, crate::ServiceError> {
        let res = self
            .client
            .post_request::<_, AggregatorResp<AggQuoteResp>>("quote", req)
            .await?;

        self.handle_result(res)
    }

    pub async fn chain_list(&self) -> Result<Vec<SupportChain>, crate::ServiceError> {
        let res = self
            .client
            .post_request::<_, AggregatorResp<Vec<SupportChain>>>("get_support_chain", "")
            .await?;

        self.handle_result(res)
    }

    pub async fn dex_list(&self, chain_id: i64) -> Result<Vec<SupportDex>, crate::ServiceError> {
        let payload = std::collections::HashMap::from([("chain_id", chain_id)]);

        let res = self
            .client
            .post_request::<_, AggregatorResp<Vec<SupportDex>>>("get_support_dex", payload)
            .await?;

        self.handle_result(res)
    }

    pub async fn token_list(
        &self,
        req: SwapTokenListReq,
    ) -> Result<serde_json::Value, crate::ServiceError> {
        let res = self
            .client
            .post_request::<_, AggregatorResp<serde_json::Value>>("get_support_dex", req)
            .await?;

        self.handle_result(res)
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportChain {
    pub chain_id: u64,
    pub chain_name: String,
    pub chain_code: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportDex {
    pub dex_id: u64,
    pub dex_name: String,
}

// 响应
#[derive(Debug, serde::Deserialize)]
pub struct AggregatorResp<T> {
    pub data: T,
    pub code: i32,
    pub msg: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct AggQuoteRequest {
    pub chain_id: u64,
    // 处理精度后的值
    pub amount: String,
    pub in_token_addr: String,
    pub out_token_addr: String,
    pub dex_ids: Vec<DexId>,
}

#[derive(Debug, serde::Serialize)]
pub struct DexId {
    pub dex_id: i32,
}

// 查询报价的响应
#[derive(Debug, serde::Deserialize)]
pub struct AggQuoteResp {
    pub chain_id: u64,
    pub amount_in: String,
    pub amount_out: String,
    pub dex_route_list: Vec<DexRoute>,
}
impl AggQuoteResp {
    pub fn amount_out_u256(&self, unit: u8) -> Result<U256, crate::ServiceError> {
        Ok(wallet_utils::unit::convert_to_u256(&self.amount_out, unit)?)
    }
}

// 路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct DexRoute {
    pub amount_in: String, // 可选择转换为 U256
    pub amount_out: String,
    pub route_in_dex: Vec<RouteInDex>,
}

// 子路由
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct RouteInDex {
    pub dex_id: u16,
    pub pool_id: String,
    pub in_token_addr: String,
    pub out_token_addr: String,
    pub zero_for_one: bool,
    pub fee: String,
    pub amount_in: String,
    pub min_amount_out: String,
}

#[cfg(test)]
mod tests {
    use super::{AggQuoteRequest, DexId, SwapClient};

    fn client() -> SwapClient {
        SwapClient::new("http://127.0.0.1:28888/api").unwrap()
    }

    #[tokio::test]
    async fn test_quote() {
        let client = client();

        let dex_id1 = DexId { dex_id: 2 };
        let dex_id2 = DexId { dex_id: 3 };

        let req = AggQuoteRequest {
            chain_id: 1,
            amount: "10000000000000000".to_string(),
            in_token_addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            out_token_addr: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
            dex_ids: vec![dex_id1, dex_id2],
        };
        let res = client.get_quote(req).await.unwrap();

        println!("res: {res:?}");
    }
}
