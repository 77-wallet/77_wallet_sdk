pub mod api_wallet;
pub mod wallet;

use crate::response::response::BackendResponse;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{collections::HashMap, fmt::Debug, sync::Arc, sync::atomic::{AtomicUsize, Ordering}};
use tokio::sync::Semaphore;

/// HostClass 用于分类主机，避免 semaphore map 无限增长
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum HostClass {
    /// 后端 API 主机
    Backend,
    /// 链 RPC 主机
    ChainRpc,
}

impl HostClass {
    /// 根据主机名分类
    fn from_host(host: &str) -> Self {
        // 检查是否是后端 API 主机
        if host.contains("api.") || host.contains("backend") || host.contains("wallet") {
            Self::Backend
        } else {
            // 其他都视为链 RPC 主机
            Self::ChainRpc
        }
    }
}

/// HTTP 任务计数器
static HTTP_PENDING_TASKS: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));

/// PendingGuard 用于确保 HTTP 任务计数准确
/// 使用 RAII 模式，确保无论任务成功还是失败，计数都会正确更新
struct HttpPendingGuard;

impl HttpPendingGuard {
    /// 创建一个新的 PendingGuard
    /// 创建时会增加计数器
    pub fn new() -> Self {
        HTTP_PENDING_TASKS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for HttpPendingGuard {
    /// 当 PendingGuard 被 drop 时，会减少计数器
    fn drop(&mut self) {
        HTTP_PENDING_TASKS.fetch_sub(1, Ordering::SeqCst);
    }
}

static HOST_LIMITERS: Lazy<DashMap<HostClass, Arc<Semaphore>>> = Lazy::new(DashMap::new);

fn host_limiter(host: &str) -> Arc<Semaphore> {
    let host_class = HostClass::from_host(host);
    HOST_LIMITERS
        .entry(host_class)
        .or_insert_with(|| Arc::new(Semaphore::new(2))) // 每个分类并发
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
        R: crate::response::BackendRespExt + Debug + Send + 'static,
        F: Fn() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<R, crate::Error>> + Send + 'static,
    {
        let start = std::time::Instant::now();
        let res = self.send_with_limit_inner(host, f).await;
        let duration = start.elapsed();
        tracing::debug!("send_with_limit {:?}, duration: {:?}", res, duration);
        res
    }

    async fn send_with_limit_inner<R, F, Fut>(&self, host: &str, f: F) -> Result<R, crate::Error>
    where
        R: crate::response::BackendRespExt + Debug,
        F: Fn() -> Fut, // ✅ Fn 而不是 FnOnce
        Fut: std::future::Future<Output = Result<R, crate::Error>>,
    {
        let h = host_limiter(host);
        
        // 监控信号量获取
        let start = std::time::Instant::now();
        let _h = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            h.acquire_owned(),
        )
        .await
        .map_err(|_| crate::Error::Timeout)??;
        let acquire_duration = start.elapsed();
        tracing::debug!("Acquired semaphore for host {}, duration: {:?}", host, acquire_duration);

        let mut backoff = 1000u64;

        for attempt in 1..=7 {
            let req_start = std::time::Instant::now();
            // 创建 HttpPendingGuard，确保任务计数准确
            let _guard = HttpPendingGuard::new();
            
            match tokio::time::timeout(
                std::time::Duration::from_secs(20),
                f(),
            ).await {
                Ok(Ok(res)) => {
                    let req_duration = req_start.elapsed();
                    tracing::debug!("HTTP request attempt {} succeeded, duration: {:?}", attempt, req_duration);
                    
                    if !res.success() && res.is_rate_limited() {
                        tracing::debug!("Rate limited response, retrying...");
                        // retry
                    } else {
                        return Ok(res);
                    }
                }
                Ok(Err(e)) => {
                    let req_duration = req_start.elapsed();
                    tracing::debug!("HTTP request attempt {} failed, duration: {:?}, error: {:?}", attempt, req_duration, e);
                    
                    if e.is_rate_limited() {
                        tracing::debug!("Rate limited error, retrying...");
                        // retry
                    } else {
                        return Err(e);
                    }
                }
                Err(_) => {
                    return Err(crate::Error::Timeout);
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            backoff = (backoff * 2).min(10_000);
            tracing::debug!("Backing off for {}ms before next attempt", backoff);
        }

        tracing::warn!("Max retry attempts reached for host {}", host);
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

    pub async fn post_default(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let host = self.base_url.clone();

        // 👇 关键：提前 clone 成 owned
        let endpoint = endpoint.to_string();
        let body = body.to_string();
        let client = self.client.clone();
        let cryptor = self.aes_cbc_cryptor.clone();

        let res: BackendResponse = self
        .send_with_limit(&host, move || {
            let endpoint = endpoint.clone();
            let body = body.clone();
            let client = client.clone();

            async move {
                Ok(client
                    .post(&endpoint)
                    .body(body)
                    .send::<BackendResponse>()
                    .await?)
            }
        })
        .await?;
        res.process::<serde_json::Value>(&cryptor)
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
        let host = self.base_url.clone();

        let endpoint = endpoint.to_string();
        let body = body.to_string();
        let client = self.client.clone();
        let cryptor = self.aes_cbc_cryptor.clone();

        let res: BackendResponse = self
        .send_with_limit(&host, move || {
            let endpoint = endpoint.clone();
            let body = body.clone();
            let client = client.clone();

            async move {
                Ok(client
                    .post(&endpoint)
                    .body(body)
                    .send::<BackendResponse>()
                    .await?)
            }
        })
        .await?;
        res.process::<T>(&cryptor)
    }

    pub async fn post_api_backend<T, R>(
        &self,
        endpoint: &str,
        req: T,
    ) -> Result<Option<R>, crate::Error>
    where
        T: serde::Serialize + Debug + Clone + Send + 'static,
        R: serde::de::DeserializeOwned + serde::Serialize,
    {
        let host = self.base_url.clone();

        let endpoint = endpoint.to_string();
        let client = self.client.clone();
        let req = req.clone();

        let res: crate::response::api_response::ApiBackendResponse = self
            .send_with_limit(&host, move || {
                let endpoint = endpoint.clone();
                let client = client.clone();
                let req = req.clone();

                async move { Ok(client.post(&endpoint).json(&req).send().await?) }
            })
            .await?;
        tracing::debug!("post_api_backend {:?}", res);
        res.process::<R>()
    }
}
