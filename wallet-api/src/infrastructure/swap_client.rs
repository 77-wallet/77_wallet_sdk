// 先放在这里，不知道最终会不会和后端用一个
use crate::request::transaction::DexRoute;
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
        res: AggregatorResp,
    ) -> Result<T, crate::ServiceError> {
        if res.code == 200 {
            Ok(wallet_utils::serde_func::serde_from_value::<T>(res.data)?)
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
            .post_request::<_, AggregatorResp>("quote", req)
            .await?;

        self.handle_result::<AggQuoteResp>(res)
    }

    pub async fn chain_list(&self) -> Result<SupportChain, crate::ServiceError> {
        let res = self
            .client
            .post_request::<_, AggregatorResp>("get_support_chain_dex", "")
            .await?;

        self.handle_result::<SupportChain>(res)
    }

    // pub async fn dex_list(&self, chain_code: &str) -> Result<Vec<SupportDex>, crate::ServiceError> {
    //     let payload = std::collections::HashMap::from([("chain_code", chain_code)]);

    //     let res = self
    //         .client
    //         .post_request::<_, AggregatorResp>("get_support_dex", payload)
    //         .await?;

    //     self.handle_result::<Vec<SupportDex>>(res)
    // }

    // pub async fn default_quote(
    //     &self,
    //     chain_code: &str,
    //     token_in: &str,
    //     token_out: &str,
    // ) -> Result<DefaultQuoteResp, crate::ServiceError> {
    //     let payload = std::collections::HashMap::from([
    //         ("chain_code", chain_code),
    //         ("in_token_addr", token_in),
    //         ("out_token_addr", token_out),
    //     ]);

    //     let res = self
    //         .client
    //         .post_request::<_, AggregatorResp>("default_quote", payload)
    //         .await?;
    //     self.handle_result::<DefaultQuoteResp>(res)
    // }

    // pub async fn swap_contract(
    //     &self,
    //     chain_code: String,
    // ) -> Result<serde_json::Value, crate::ServiceError> {
    //     let payload = std::collections::HashMap::from([("chain_code", chain_code)]);
    //     let res = self
    //         .client
    //         .post_request::<_, AggregatorResp>("get_swap_contract_address", payload)
    //         .await?;
    //     self.handle_result::<serde_json::Value>(res)
    // }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SupportChain {
    pub chain_dexs: Vec<ChainDex>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainDex {
    pub chain_code: String,
    #[serde(alias = "swapContractAddr")]
    pub aggregator_addr: String,
    pub dexs: Vec<DexInfo>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DexInfo {
    pub dex_id: u64,
    pub dex_name: String,
}

// 响应
#[derive(Debug, serde::Deserialize)]
pub struct AggregatorResp {
    pub data: serde_json::Value,
    pub code: i32,
    pub msg: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct AggQuoteRequest {
    pub chain_code: String,
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
    pub chain_code: String,
    pub amount_in: String,
    pub amount_out: String,
    pub dex_route_list: Vec<DexRoute>,
    pub default_slippage: u64,
}
impl AggQuoteResp {
    pub fn amount_out_u256(&self) -> Result<U256, crate::ServiceError> {
        Ok(wallet_utils::unit::u256_from_str(&self.amount_out)?)
    }

    pub fn get_slippage(&self) -> f64 {
        self.default_slippage as f64 / 10000.0
    }
}

// 默认的兑换比例
#[derive(serde::Serialize)]
pub struct DefaultQuoteResp {
    pub token_in: crate::response_vo::swap::SwapTokenInfo,
    pub token_out: crate::response_vo::swap::SwapTokenInfo,
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
            chain_code: "eth".to_string(),
            amount: "10000000000000000".to_string(),
            in_token_addr: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
            out_token_addr: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string(),
            dex_ids: vec![dex_id1, dex_id2],
        };
        let res = client.get_quote(req).await.unwrap();

        println!("res: {res:?}");
    }
}
