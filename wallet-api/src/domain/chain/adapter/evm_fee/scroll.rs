use crate::error::service::ServiceError;
use alloy::{primitives::U256, rpc::types::TransactionRequest};
use serde_json::json;
use std::{collections::HashMap, time::Duration};
use wallet_transport::{client::RpcClient, types::JsonRpcParams};
use wallet_utils::unit;

pub(super) async fn estimate_l1_data_fee(
    rpc_url: &str,
    headers: Option<HashMap<String, String>>,
    tx: TransactionRequest,
) -> Result<U256, ServiceError> {
    let client =
        RpcClient::new(rpc_url, headers, Some(Duration::from_secs(super::super::TIME_OUT)))?;
    let params = JsonRpcParams::default()
        .method("scroll_estimateL1DataFee")
        .params(vec![json!(tx), json!("latest")]);
    let fee: String = client.invoke_request(params).await?;
    let fee = unit::u256_from_str(&fee)?;
    if fee == U256::ZERO {
        return Err(ServiceError::Context("Scroll L1 data fee RPC returned zero".to_string()));
    }
    Ok(fee)
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
