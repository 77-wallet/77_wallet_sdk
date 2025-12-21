pub mod api_wallet;
pub mod wallet;

use crate::response::response::BackendResponse;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use tokio::sync::Semaphore;

static GLOBAL_LIMITER: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(20))); // 全局并发

static HOST_LIMITERS: Lazy<DashMap<String, Arc<Semaphore>>> = Lazy::new(DashMap::new);

fn host_limiter(host: &str) -> Arc<Semaphore> {
    HOST_LIMITERS
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(10))) // 每个域名10并发
        .clone()
}

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

    async fn send_with_limit<R, F, Fut>(&self, host: &str, f: F) -> Result<R, crate::Error>
    where
        R: crate::response::BackendRespExt + Debug,
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<R, crate::Error>>,
    {
        let _g = GLOBAL_LIMITER.acquire().await?;
        let h = host_limiter(host);
        let _h = h.acquire().await?;

        let mut backoff = 500u64; //ms

        for attempt in 1..=7 {
            let start = std::time::Instant::now();
            match f().await {
                Ok(res) => {
                    tracing::info!(
                        host,
                        attempt,
                        cost_ms = start.elapsed().as_millis(),
                        code = ?res.code(),
                        "request finished"
                    );

                    if !res.success() && res.is_rate_limited() {
                        tracing::warn!(
                            host,
                            attempt,
                            "rate limited in response, backing off {} ms",
                            backoff
                        );
                    } else {
                        return Ok(res);
                    }
                }
                Err(e) => {
                    // 👇 关键：这里识别 transport 层抛出来的 429
                    if e.is_rate_limited() {
                        tracing::warn!(
                            host,
                            attempt,
                            err = %e,
                            "rate limited as error, backing off {} ms",
                            backoff
                        );
                    } else {
                        tracing::error!(
                            host,
                            attempt,
                            err = %e,
                            "request failed (non-retriable)"
                        );
                        return Err(e);
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            backoff = (backoff * 2).min(10_000);
        }

        tracing::error!(host, "exceeded max retries, still rate limited");
        Err(crate::Error::RateLimited)
    }

    pub fn replace_base_url(&mut self, base_url: &str) {
        self.base_url = base_url.to_string();
        self.client.replace_base_url(base_url);
    }

    pub async fn post_request<T, R>(&self, endpoint: &str, req: T) -> Result<R, crate::Error>
    where
        T: serde::Serialize + std::fmt::Debug,
        R: serde::de::DeserializeOwned + serde::Serialize + Debug,
    {
        let res = self.client.post(endpoint).json(req).send::<BackendResponse>().await?;
        res.process::<R>(&self.aes_cbc_cryptor)
    }

    // 发送一个字符串的请求.
    pub async fn post_req_string<T>(&self, endpoint: &str, body: String) -> Result<T, crate::Error>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Debug,
    {
        let res = self.client.post(endpoint).body(body).send::<BackendResponse>().await?;
        res.process::<T>(&self.aes_cbc_cryptor)
    }

    // 发送一个字符串的请求.
    pub async fn post_req_str<T>(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<T, crate::Error>
    where
        T: serde::de::DeserializeOwned + serde::Serialize + Debug,
    {
        let res =
            self.client.post(endpoint).body(body.to_string()).send::<BackendResponse>().await?;
        res.process::<T>(&self.aes_cbc_cryptor)
    }

    pub async fn post_api_backend<T, R>(
        &self,
        endpoint: &str,
        req: T,
    ) -> Result<Option<R>, crate::Error>
    where
        T: serde::Serialize + Debug,
        R: serde::de::DeserializeOwned + serde::Serialize,
    {
        let host = self.base_url.clone();

        let res: crate::response::api_response::ApiBackendResponse = self
            .send_with_limit(&host, || async {
                Ok(self.client.post(endpoint).json(&req).send().await?)
            })
            .await?;

        res.process::<R>()
    }
}
