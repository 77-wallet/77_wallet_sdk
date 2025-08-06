use crate::response::BackendResponse;
use std::collections::HashMap;

pub mod address;
pub mod announcement;
pub mod api_wallet;
pub mod app;
pub mod chain;
pub mod coin;
pub mod config;
pub mod device;
pub mod mqtt;
pub mod permission;
pub mod signed;
pub mod stake;
pub mod swap;
pub mod transaction;

#[derive(Debug, Clone)]
pub struct BackendApi {
    pub base_url: String,
    pub client: wallet_transport::client::HttpClient,
    pub(crate) aes_cbc_cryptor: wallet_utils::cbc::AesCbcCryptor,
}

impl BackendApi {
    pub fn new(
        backend_url: Option<String>,
        headers_opt: Option<HashMap<String, String>>,
        aes_cbc_cryptor: wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<Self, crate::Error> {
        let url = backend_url.unwrap_or(crate::consts::BASE_URL.to_string());

        let mut headers_opt = headers_opt.unwrap_or_default();
        headers_opt.insert("Accept-Encoding".to_string(), "identity".to_string());

        let timeout = Some(std::time::Duration::from_secs(15));
        Ok(Self {
            base_url: url.to_string(),
            client: wallet_transport::client::HttpClient::new(&url, Some(headers_opt), timeout)?,
            aes_cbc_cryptor,
        })
    }

    pub fn replace_base_url(&mut self, base_url: &str) {
        self.base_url = base_url.to_string();
        self.client.replace_base_url(base_url);
    }

    pub async fn post_request<T, R>(&self, endpoint: &str, req: T) -> Result<R, crate::Error>
    where
        T: serde::Serialize + std::fmt::Debug,
        R: serde::de::DeserializeOwned + serde::Serialize,
    {
        let res = self
            .client
            .post(endpoint)
            .json(req)
            .send::<BackendResponse>()
            .await?;
        res.process::<R>(&self.aes_cbc_cryptor)
    }

    // 发送一个字符串的请求.
    pub async fn post_req_string<T>(&self, endpoint: &str, body: String) -> Result<T, crate::Error>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
    {
        let res = self
            .client
            .post(endpoint)
            .body(body)
            .send::<BackendResponse>()
            .await?;
        res.process::<T>(&self.aes_cbc_cryptor)
    }

    // 发送一个字符串的请求.
    pub async fn post_req_str<T>(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<T, crate::Error>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
    {
        let res = self
            .client
            .post(endpoint)
            .body(body.to_string())
            .send::<BackendResponse>()
            .await?;
        res.process::<T>(&self.aes_cbc_cryptor)
    }
}
