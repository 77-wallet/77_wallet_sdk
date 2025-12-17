use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::{
    sync::{
        mpsc::{self},
        oneshot,
    },
    time::interval,
};
use wallet_chain_interact::types::ChainPrivateKey;

use crate::{
    domain::api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    error::service::ServiceError,
};
use tracing::{error, info};

/// 私钥请求消息
#[derive(Debug)]
enum PrivateKeyCmd {
    Get {
        address: String,
        chain_code: String,
        password: String,
        resp: tokio::sync::oneshot::Sender<Result<ChainPrivateKey, ServiceError>>,
    },
    InsertResult {
        address: String,
        chain_code: String,
        result: Result<ChainPrivateKey, ServiceError>,
    },
    Preload {
        address: String,
        chain_code: String,
    },
    Insert {
        address: String,
        chain_code: String,
        private_key: ChainPrivateKey,
        ttl: Duration,
    },
}

type CacheKey = (String, String);

/// 私钥项，包含私钥和过期时间
#[derive(Debug)]
struct CacheItem {
    key: ChainPrivateKey,
    expires_at: Instant,
}

pub struct PrivateKeyActor {
    rx: mpsc::Receiver<PrivateKeyCmd>,
    tx: mpsc::Sender<PrivateKeyCmd>,
    cache: HashMap<CacheKey, CacheItem>,
    inflight: HashMap<CacheKey, Vec<oneshot::Sender<Result<ChainPrivateKey, ServiceError>>>>,
}

impl PrivateKeyActor {
    async fn run(mut self) {
        info!("PrivateKeyActor started, beginning to process private key commands");
        let mut clean_tick = interval(Duration::from_secs(24 * 60 * 60));

        loop {
            tokio::select! {
                Some(cmd) = self.rx.recv() => {
                    self.handle_cmd(cmd).await;
                }
                _ = clean_tick.tick() => {
                    self.clean_expired();
                }
            }
        }
    }

    async fn handle_cmd(&mut self, cmd: PrivateKeyCmd) {
        match cmd {
            PrivateKeyCmd::Get { address, chain_code, password, resp } => {
                let key = (address.clone(), chain_code.clone());
                info!(address = %address, chain_code = %chain_code, "Received Get private key command");

                // 1. cache hit
                if let Some(item) = self.cache.get(&key) {
                    if item.expires_at > Instant::now() {
                        info!(address = %address, chain_code = %chain_code, "Cache hit for private key");
                        let _ = resp.send(Ok(item.key.clone()));
                        return;
                    }
                }

                // 2. 已有 inflight
                if let Some(waiters) = self.inflight.get_mut(&key) {
                    info!(address = %address, chain_code = %chain_code, "Inflight request found, adding to waiters list");
                    waiters.push(resp);
                    return;
                }

                // 3. 第一个 miss
                info!(address = %address, chain_code = %chain_code, "Cache miss, initiating private key retrieval");
                self.inflight.insert(key.clone(), vec![resp]);

                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let result =
                        ApiAccountDomain::get_private_key(&address, &chain_code, &password).await;

                    let _ =
                        tx.send(PrivateKeyCmd::InsertResult { address, chain_code, result }).await;
                });
            }
            PrivateKeyCmd::InsertResult { address, chain_code, result } => {
                let key = (address.clone(), chain_code.clone());
                info!(address = %address, chain_code = %chain_code, "Received InsertResult command");

                if let Some(waiters) = self.inflight.remove(&key) {
                    info!(address = %address, chain_code = %chain_code, waiters_count = %waiters.len(), "Processing result for inflight requests");
                    match result {
                        Ok(pk) => {
                            // 写 cache（只一次）
                            self.cache.insert(
                                key,
                                CacheItem {
                                    key: pk.clone(),
                                    expires_at: Instant::now() + Duration::from_secs(10 * 60),
                                },
                            );
                            info!(address = %address, chain_code = %chain_code, "Private key cached successfully");

                            // 每个 waiter 拿 clone 的 pk
                            for resp in waiters {
                                let _ = resp.send(Ok(pk.clone()));
                            }
                            info!(address = %address, chain_code = %chain_code, "Distributed private key to all waiters");
                        }
                        Err(err) => {
                            error!(address = %address, chain_code = %chain_code, err = %err, "Failed to get private key");
                            let mut it = waiters.into_iter();
                            if let Some(last) = it.next_back() {
                                for resp in it {
                                    let _ = resp.send(Err(ServiceError::System(
                                        crate::error::system::SystemError::Internal(
                                            err.to_string(),
                                        ),
                                    )));
                                }
                                let _ = last.send(Err(err));
                            }
                            info!(address = %address, chain_code = %chain_code, "Distributed error to all waiters");
                        }
                    }
                }
            }
            PrivateKeyCmd::Preload { address, chain_code } => {
                let tx = self.tx.clone();
                info!(address = %address, chain_code = %chain_code, "Received Preload private key command");

                tokio::spawn(async move {
                    info!(address = %address, chain_code = %chain_code, "Starting async preload task");
                    match ApiWalletDomain::get_passwd().await {
                        Ok(password) => {
                            match ApiAccountDomain::get_private_key(
                                &address,
                                &chain_code,
                                &password,
                            )
                            .await
                            {
                                Ok(private_key) => {
                                    let _ = tx
                                        .send(PrivateKeyCmd::Insert {
                                            address: address.clone(),
                                            chain_code: chain_code.clone(),
                                            private_key,
                                            ttl: Duration::from_secs(3 * 60 * 60),
                                        })
                                        .await;
                                    info!(address = %address, chain_code = %chain_code, "Preload completed successfully");
                                }
                                Err(e) => {
                                    error!(address = %address, chain_code = %chain_code, err = %e, "Preload failed to get private key");
                                }
                            }
                        }
                        Err(e) => {
                            error!(address = %address, chain_code = %chain_code, err = %e, "Preload failed to get password")
                        }
                    }
                });
            }
            PrivateKeyCmd::Insert { address, chain_code, private_key, ttl } => {
                let key = (address.clone(), chain_code.clone());
                info!(address = %address, chain_code = %chain_code, ttl_seconds = %ttl.as_secs(), "Received Insert private key command");

                if let Some(old) = self.cache.get(&key) {
                    if old.expires_at > Instant::now() {
                        info!(address = %address, chain_code = %chain_code, "Existing valid cache found, skipping insert");
                        return;
                    }
                    info!(address = %address, chain_code = %chain_code, "Found expired cache, will replace");
                }

                self.cache
                    .insert(key, CacheItem { key: private_key, expires_at: Instant::now() + ttl });

                info!(address = %address, chain_code = %chain_code, ttl_seconds = %ttl.as_secs(), "Private key cached successfully via insert");
            }
        }
    }

    fn clean_expired(&mut self) {
        let now = Instant::now();
        let before_count = self.cache.len();
        self.cache.retain(|_, v| v.expires_at > now);
        let after_count = self.cache.len();
        let cleaned_count = before_count - after_count;
        info!(before_count = %before_count, after_count = %after_count, cleaned_count = %cleaned_count, "Private key cache cleaned, removed expired items");
    }
}

/// 基于 channel 的私钥管理器
#[derive(Debug, Clone)]
pub struct PrivateKeyManager {
    tx: mpsc::Sender<PrivateKeyCmd>,
}

impl PrivateKeyManager {
    /// 创建新的私钥管理器实例
    pub fn start() -> Self {
        info!("Creating new PrivateKeyManager instance");
        let (tx, rx) = mpsc::channel(128);

        let actor =
            PrivateKeyActor { rx, tx: tx.clone(), cache: HashMap::new(), inflight: HashMap::new() };
        tokio::spawn(actor.run());

        info!("PrivateKeyManager started successfully");
        Self { tx }
    }

    /// 获取私钥
    pub async fn get_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, ServiceError> {
        info!(address = %address, chain_code = %chain_code, "Public API: get_private_key called");
        let (resp_tx, resp_rx) = oneshot::channel();

        self.tx
            .send(PrivateKeyCmd::Get {
                address: address.to_string(),
                chain_code: chain_code.to_string(),
                password: password.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| {
                error!(address = %address, chain_code = %chain_code, "Failed to send Get command to PrivateKeyActor");
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "PrivateKeyActor not running".into(),
                ))
            })?;

        let result = resp_rx.await.map_err(|_| {
            error!(address = %address, chain_code = %chain_code, "PrivateKeyActor dropped response channel");
            ServiceError::System(crate::error::system::SystemError::Internal(
                "PrivateKeyActor dropped response".into(),
            ))
        })?;

        match &result {
            Ok(_) => {
                info!(address = %address, chain_code = %chain_code, "Public API: get_private_key completed successfully")
            }
            Err(err) => {
                error!(address = %address, chain_code = %chain_code, err = %err, "Public API: get_private_key failed")
            }
        }

        result
    }

    /// 存储私钥
    pub async fn preload(&self, address: &str, chain_code: &str) -> Result<(), ServiceError> {
        info!(address = %address, chain_code = %chain_code, "Public API: preload called");
        // 发送存储私钥请求
        let message = PrivateKeyCmd::Preload {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
        };

        if let Err(e) = self.tx.send(message).await {
            error!(address = %address, chain_code = %chain_code, err = %e, "Failed to send Preload command to PrivateKeyActor");
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                format!("Failed to send store private key request: {:?}", e),
            )));
        }
        info!(address = %address, chain_code = %chain_code, "Public API: preload command sent successfully");
        Ok(())
    }
}
