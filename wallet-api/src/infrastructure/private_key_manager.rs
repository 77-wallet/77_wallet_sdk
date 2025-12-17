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
}

impl PrivateKeyActor {
    async fn run(mut self) {
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

                if let Some(item) = self.cache.get(&key) {
                    if item.expires_at > Instant::now() {
                        info!(address = %address, chain = %chain_code, "private key cache hit");
                        let _ = resp.send(Ok(item.key.clone()));
                        return;
                    }
                }

                let result = ApiAccountDomain::get_private_key(&address, &chain_code, &password)
                    .await
                    .map(|pk| {
                        self.cache.insert(
                            key,
                            CacheItem {
                                key: pk.clone(),
                                expires_at: Instant::now() + Duration::from_secs(10 * 60),
                            },
                        );
                        pk
                    });
                let _ = resp.send(result);
            }

            PrivateKeyCmd::Preload { address, chain_code } => {
                let tx = self.tx.clone();

                tokio::spawn(async move {
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
                                            address,
                                            chain_code,
                                            private_key,
                                            ttl: Duration::from_secs(3 * 60 * 60),
                                        })
                                        .await;
                                }
                                Err(e) => {
                                    error!("preload get private key failed: {:?}", e);
                                }
                            }
                        }
                        Err(e) => error!("get passwd failed: {:?}", e),
                    }
                });
            }
            PrivateKeyCmd::Insert { address, chain_code, private_key, ttl } => {
                let key = (address, chain_code);

                if let Some(old) = self.cache.get(&key) {
                    if old.expires_at > Instant::now() {
                        return;
                    }
                }

                self.cache
                    .insert(key, CacheItem { key: private_key, expires_at: Instant::now() + ttl });

                info!("private key cached via insert");
            }
        }
    }

    fn clean_expired(&mut self) {
        let now = Instant::now();
        self.cache.retain(|_, v| v.expires_at > now);
        info!("private key cache cleaned");
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
        let (tx, rx) = mpsc::channel(128);

        let actor = PrivateKeyActor { rx, tx: tx.clone(), cache: HashMap::new() };
        tokio::spawn(actor.run());

        Self { tx }
    }

    /// 获取私钥
    pub async fn get_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, ServiceError> {
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
                ServiceError::System(crate::error::system::SystemError::Internal(
                    "PrivateKeyActor not running".into(),
                ))
            })?;

        resp_rx.await.map_err(|_| {
            ServiceError::System(crate::error::system::SystemError::Internal(
                "PrivateKeyActor dropped response".into(),
            ))
        })?
    }

    /// 存储私钥
    pub async fn preload(&self, address: &str, chain_code: &str) -> Result<(), ServiceError> {
        // 发送存储私钥请求
        let message = PrivateKeyCmd::Preload {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
        };

        if let Err(e) = self.tx.send(message).await {
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                format!("Failed to send store private key request: {:?}", e),
            )));
        }
        Ok(())
    }
}
