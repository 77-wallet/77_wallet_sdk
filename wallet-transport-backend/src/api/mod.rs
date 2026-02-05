pub mod api_wallet;
pub mod wallet;

use crate::response::response::BackendResponse;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use tokio::sync::Semaphore;

use futures::future::BoxFuture;
use tokio::sync::{mpsc, oneshot};

struct Job {
    fut: BoxFuture<'static, ()>,
}

/// worker 数 = 10~15   // 全局真实并发
/// 单 host 并发 = 5~8
/// 队列 mpsc = 2000    // 缓冲洪峰
static ENTRY_LIMITER: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(50))); // 👈 SDK 总在路上任务数

static CPU_LIMITER: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(1))); // 👈 手机建议 2~3
static REQUEST_TX: Lazy<mpsc::Sender<Job>> = Lazy::new(|| {
    let (tx, rx) = mpsc::channel::<Job>(200);

    let rx = Arc::new(tokio::sync::Mutex::new(rx));

    let worker_num = 2; // 手机上建议 1~2
    for id in 0..worker_num {
        let rx = rx.clone();
        tokio::spawn(async move {
            loop {
                let job = {
                    let mut guard = rx.lock().await;
                    guard.recv().await
                };

                match job {
                    Some(job) => {
                        job.fut.await;
                    }
                    None => {
                        tracing::debug!(worker = id, "worker exited");
                        break;
                    }
                }
            }
        });
    }

    tx
});

static HOST_LIMITERS: Lazy<DashMap<String, Arc<Semaphore>>> = Lazy::new(DashMap::new);

fn host_limiter(host: &str) -> Arc<Semaphore> {
    HOST_LIMITERS
        .entry(host.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(2))) // 每个域名10并发
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
        let (tx, rx) = oneshot::channel();
        let api = self.clone();
        let host = host.to_string();

        let fut = Box::pin(async move {
            let start = std::time::Instant::now();
            let res = api.send_with_limit_inner(&host, f).await;
            let duration = start.elapsed();
            tracing::debug!("send_with_limit_inner {:?}, duration: {:?}", res, duration);
            let _ = tx.send(res);
        });

        // 非阻塞发送，避免队列满时阻塞
        REQUEST_TX.try_send(Job { fut }).map_err(|e| {
            match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!("Request queue is full, returning backpressure");
                    crate::Error::Backpressure
                }
                _ => {
                    tracing::warn!("Failed to send request to queue: {:?}", e);
                    crate::Error::RateLimited
                }
            }
        })?;

        rx.await.map_err(|_| {
            tracing::warn!("Request channel closed unexpectedly");
            crate::Error::RateLimited
        })?
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
        let _h = h.acquire_owned().await?;
        let acquire_duration = start.elapsed();
        tracing::debug!("Acquired semaphore for host {}, duration: {:?}", host, acquire_duration);

        let mut backoff = 1000u64;

        for attempt in 1..=7 {
            let req_start = std::time::Instant::now();
            match f().await {
                // ✅ 每次调用都 OK
                Ok(res) => {
                    let req_duration = req_start.elapsed();
                    tracing::debug!("HTTP request attempt {} succeeded, duration: {:?}", attempt, req_duration);
                    
                    if !res.success() && res.is_rate_limited() {
                        tracing::debug!("Rate limited response, retrying...");
                        // retry
                    } else {
                        return Ok(res);
                    }
                }
                Err(e) => {
                    let req_duration = req_start.elapsed();
                    tracing::debug!("HTTP request attempt {} failed, duration: {:?}, error: {:?}", attempt, req_duration, e);
                    
                    if e.is_rate_limited() {
                        tracing::debug!("Rate limited error, retrying...");
                        // retry
                    } else {
                        return Err(e);
                    }
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
