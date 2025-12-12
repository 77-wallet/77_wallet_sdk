use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{
        Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    time::sleep,
};
use wallet_chain_interact::types::ChainPrivateKey;

use crate::{domain::api_wallet::account::ApiAccountDomain, error::service::ServiceError};
use tracing::{error, info};

/// 私钥请求消息
#[derive(Debug)]
enum PrivateKeyMessage {
    GetPrivateKey {
        address: String,
        chain_code: String,
        password: String,
        response_tx: Sender<Result<ChainPrivateKey, ServiceError>>,
    },
    StorePrivateKey {
        address: String,
        chain_code: String,
    },
    ClearExpired,
    Shutdown,
}

/// 私钥项，包含私钥和过期时间
#[derive(Debug)]
struct PrivateKeyItem {
    private_key: ChainPrivateKey,
    expires_at: Instant,
}

/// 基于 channel 的私钥管理器
#[derive(Debug, Clone)]
pub struct PrivateKeyManager {
    message_tx: Sender<PrivateKeyMessage>,
}

impl PrivateKeyManager {
    /// 创建新的私钥管理器实例
    pub async fn new() -> Result<Self, ServiceError> {
        let (message_tx, message_rx) = channel(100);

        // 启动私钥管理器后台任务
        tokio::spawn(Self::run_manager(message_rx));

        Ok(Self { message_tx })
    }

    /// 关闭私钥管理器
    pub async fn close(&self) -> Result<(), ServiceError> {
        // 发送关闭请求
        let message = PrivateKeyMessage::Shutdown;

        if self.message_tx.send(message).await.is_err() {
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "Failed to send shutdown request".to_string(),
            )));
        }

        Ok(())
    }

    /// 运行私钥管理器
    async fn run_manager(mut message_rx: Receiver<PrivateKeyMessage>) {
        // 存储私钥的哈希表，键为 (address, chain_code)
        // 使用 Arc<Mutex<>> 包装，以便在异步任务中安全地访问和更新
        let private_key_map =
            Arc::new(Mutex::new(HashMap::<(String, String), PrivateKeyItem>::new()));

        // 启动过期清理定时器
        let (cleanup_tx, mut cleanup_rx) = channel(1);
        tokio::spawn(Self::run_cleanup_timer(cleanup_tx));

        info!("Private key manager started");

        loop {
            tokio::select! {
                // 处理私钥请求
                Some(msg) = message_rx.recv() => {
                    match msg {
                        PrivateKeyMessage::GetPrivateKey {
                            address,
                            chain_code,
                            password,
                            response_tx,
                        } => {
                            let key = (address.clone(), chain_code.clone());
                            let private_key_map_clone = private_key_map.clone();

                            // 检查私钥是否在缓存中且未过期
                            {
                                let private_key_map_guard = private_key_map_clone.lock().await;
                                if let Some(item) = private_key_map_guard.get(&key) {
                                    if item.expires_at > Instant::now() {
                                        // 使用缓存的私钥
                                        info!("Using cached private key for address: {}", address);
                                        let _ = response_tx.send(Ok(item.private_key.clone())).await;
                                        continue;
                                    } else {
                                        // 私钥已过期，移除缓存
                                        info!("Private key expired for address: {}, removing from cache", address);
                                    }
                                }
                            }

                            // 从数据库获取私钥
                            info!("Getting private key from database for address: {}", address);
                            let result = ApiAccountDomain::get_private_key(&address, &chain_code, &password)
                                .await
                                .map(|private_key| {
                                    // 缓存私钥，设置 10 分钟过期
                                    let expires_at = Instant::now() + Duration::from_secs(10 * 60);
                                    let mut private_key_map_guard = private_key_map_clone.blocking_lock();
                                    private_key_map_guard.insert(key, PrivateKeyItem {
                                        private_key: private_key.clone(),
                                        expires_at,
                                    });
                                    private_key
                                });

                            // 发送响应
                            let _ = response_tx.send(result).await;
                        },
                        PrivateKeyMessage::StorePrivateKey {
                            address,
                            chain_code,
                        } => {
                            // 克隆私钥缓存，以便在异步任务中使用
                            let private_key_map_clone = private_key_map.clone();

                            // 创建一个异步任务来处理存储私钥的请求，避免堵塞主线程
                            tokio::spawn(async move {
                                // 获取密码
                                match crate::domain::api_wallet::wallet::ApiWalletDomain::get_passwd().await {
                                    Ok(password) => {
                                        // 从数据库获取私钥
                                        match ApiAccountDomain::get_private_key(&address, &chain_code, &password).await {
                                            Ok(private_key) => {
                                                // 缓存私钥，设置3小时过期
                                                let expires_at = Instant::now() + Duration::from_secs(3 * 60 * 60);
                                                let key = (address.clone(), chain_code.clone());

                                                // 更新私钥缓存
                                                let mut private_key_map_guard = private_key_map_clone.lock().await;
                                                private_key_map_guard.insert(key, PrivateKeyItem {
                                                    private_key,
                                                    expires_at,
                                                });

                                                info!("Private key stored for address: {}", address);
                                            },
                                            Err(e) => {
                                                error!("Failed to get private key for storage, address: {}, chain_code: {}, error: {:?}", address, chain_code, e);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        error!("Failed to get password for private key storage, address: {}, chain_code: {}, error: {:?}", address, chain_code, e);
                                    }
                                }
                            });
                        },
                        PrivateKeyMessage::ClearExpired => {
                            // 清理过期的私钥
                            let now = Instant::now();
                            let mut private_key_map_guard = private_key_map.lock().await;
                            private_key_map_guard
                                .retain(|_, item| item.expires_at > now);
                            info!("Cleaned up expired private keys");
                        },
                        PrivateKeyMessage::Shutdown => {
                            // 关闭私钥管理器
                            info!("Private key manager shutting down");
                            break;
                        },
                    }
                },
                // 处理清理定时器消息
                _ = cleanup_rx.recv() => {
                    // 清理过期的私钥
                    let now = Instant::now();
                    let mut private_key_map_guard = private_key_map.lock().await;
                    private_key_map_guard
                        .retain(|_, item| item.expires_at > now);
                    info!("Cleaned up expired private keys");
                },
            }
        }
    }

    /// 运行清理定时器
    async fn run_cleanup_timer(cleanup_tx: Sender<PrivateKeyMessage>) {
        loop {
            // 每 24 小时清理一次过期的私钥
            sleep(Duration::from_secs(24 * 60 * 60)).await;

            if cleanup_tx.send(PrivateKeyMessage::ClearExpired).await.is_err() {
                // 通道已关闭，退出定时器
                break;
            }
        }
    }

    /// 获取私钥
    pub async fn get_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, ServiceError> {
        let (response_tx, mut response_rx) = channel(1);

        // 发送获取私钥请求
        let message = PrivateKeyMessage::GetPrivateKey {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            password: password.to_string(),
            response_tx,
        };

        if self.message_tx.send(message).await.is_err() {
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "Failed to send private key request".to_string(),
            )));
        }

        // 等待响应
        match response_rx.recv().await {
            Some(result) => result,
            None => Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "Private key manager closed unexpectedly".to_string(),
            ))),
        }
    }

    /// 存储私钥
    pub async fn store_private_key(
        &self,
        address: &str,
        chain_code: &str,
    ) -> Result<(), ServiceError> {
        // 发送存储私钥请求
        let message = PrivateKeyMessage::StorePrivateKey {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
        };

        if self.message_tx.send(message).await.is_err() {
            return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                "Failed to send store private key request".to_string(),
            )));
        }
        Ok(())
    }
}
