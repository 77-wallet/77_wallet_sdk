use wallet_database::repositories::api_wallet::{
    collect_strategy::ApiCollectStrategyRepo,
    collect_strategy_chain_config::ApiCollectStrategyChainConfigRepo,
    withdraw_strategy::ApiWithdrawStrategyRepo,
    withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigRepo,
};
use wallet_transport_backend::request::api_wallet::strategy::{
    ChainConfig, IndexAndAddress, Strategy,
};

pub(crate) struct StrategyDomain {}

impl StrategyDomain {
    pub async fn query_collect_strategy(
        &self,
        uid: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::strategy::Strategy,
        crate::error::service::ServiceError,
    > {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 先尝试从本地数据库查询
        if let Some(local_strategy) = ApiCollectStrategyRepo::get_by_uid(&pool, uid).await? {
            // 查询链配置
            let chain_configs =
                ApiCollectStrategyChainConfigRepo::get_by_strategy_id(&pool, local_strategy.id)
                    .await?;

            // 转换为 Strategy 类型
            let chain_configs = chain_configs
                .into_iter()
                .map(|config| ChainConfig {
                    chain_code: config.chain_code,
                    chain_address_type: config.chain_address_type,
                    normal_address: IndexAndAddress {
                        index: config.normal_idx,
                        address: config.normal_address,
                    },
                    risk_address: IndexAndAddress {
                        index: config.risk_idx,
                        address: config.risk_address,
                    },
                })
                .collect();

            return Ok(Strategy {
                uid: local_strategy.uid,
                threshold: local_strategy.threshold as u32,
                chain_configs,
            });
        }

        // 2. 本地没有则从后端查询
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api().clone();
        let backend_resp = backend_api.query_collect_strategy(uid).await?;

        // 3. 将后端结果保存到本地数据库
        self.save_collect_strategy_from_backend(uid, &backend_resp).await?;

        // 4. 转换为 Strategy 类型并返回
        let chain_configs = backend_resp
            .chain_configs
            .into_iter()
            .map(|config| ChainConfig {
                chain_code: config.chain_code,
                chain_address_type: config.chain_address_type,
                normal_address: IndexAndAddress {
                    index: config.normal_address.index,
                    address: config.normal_address.address,
                },
                risk_address: IndexAndAddress {
                    index: config.risk_address.index,
                    address: config.risk_address.address,
                },
            })
            .collect();

        Ok(Strategy { uid: uid.to_string(), threshold: backend_resp.threshold, chain_configs })
    }

    async fn save_collect_strategy_from_backend(
        &self,
        uid: &str,
        backend_resp: &wallet_transport_backend::response_vo::api_wallet::strategy::CollectionStrategyResp,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 保存主策略
        let strategy_entity =
            wallet_database::entities::api_collect_strategy::ApiCollectStrategyEntity {
                id: 0, // 自增ID，插入时会自动生成
                uid: uid.to_string(),
                threshold: backend_resp.threshold,
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

        // 使用upsert保存策略
        ApiCollectStrategyRepo::upsert(&pool, strategy_entity).await?;

        // 2. 获取刚插入的策略ID
        let strategy = ApiCollectStrategyRepo::get_by_uid(&pool, uid).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                        "保存策略失败".to_owned(),
                    ),
                ),
            ),
        )?;

        // 3. 保存链配置
        for config in &backend_resp.chain_configs {
            let chain_config_entity = wallet_database::entities::api_collect_strategy_chain_config::ApiCollectStrategyChainConfigEntity {
                id: 0, // 自增ID
                strategy_id: strategy.id,
                chain_code: config.chain_code.clone(),
                chain_address_type: config.chain_address_type.clone(),
                normal_idx: config.normal_address.index,
                normal_address: config.normal_address.address.clone(),
                risk_idx: config.risk_address.index,
                risk_address: config.risk_address.address.clone(),
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

            ApiCollectStrategyChainConfigRepo::upsert(&pool, chain_config_entity).await?;
        }

        Ok(())
    }

    pub async fn save_local_collect_strategy(
        &self,
        uid: &str,
        strategy: &wallet_transport_backend::request::api_wallet::strategy::Strategy,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 保存主策略
        let strategy_entity =
            wallet_database::entities::api_collect_strategy::ApiCollectStrategyEntity {
                id: 0, // 自增ID
                uid: uid.to_string(),
                threshold: strategy.threshold,
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

        // 使用upsert保存策略
        ApiCollectStrategyRepo::upsert(&pool, strategy_entity).await?;

        // 2. 获取刚插入的策略ID
        let saved_strategy = ApiCollectStrategyRepo::get_by_uid(&pool, uid).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                        "保存策略失败".to_owned(),
                    ),
                ),
            ),
        )?;

        // 3. 先删除旧的链配置
        ApiCollectStrategyChainConfigRepo::delete_by_strategy_id(&pool, saved_strategy.id).await?;

        // 4. 保存新的链配置
        for config in &strategy.chain_configs {
            let chain_config_entity = wallet_database::entities::api_collect_strategy_chain_config::ApiCollectStrategyChainConfigEntity {
                id: 0, // 自增ID
                strategy_id: saved_strategy.id,
                chain_code: config.chain_code.clone(),
                chain_address_type: config.chain_address_type.clone(),
                normal_idx: config.normal_address.index,
                normal_address: config.normal_address.address.clone(),
                risk_idx: config.risk_address.index,
                risk_address: config.risk_address.address.clone(),
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

            ApiCollectStrategyChainConfigRepo::upsert(&pool, chain_config_entity).await?;
        }

        Ok(())
    }

    pub async fn query_withdraw_strategy(
        &self,
        uid: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::strategy::Strategy,
        crate::error::service::ServiceError,
    > {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 先尝试从本地数据库查询
        if let Some(local_strategy) = ApiWithdrawStrategyRepo::get_by_uid(&pool, uid).await? {
            // 查询链配置
            let chain_configs =
                ApiWithdrawStrategyChainConfigRepo::get_by_strategy_id(&pool, local_strategy.id)
                    .await?;

            // 转换为 Strategy 类型
            let chain_configs = chain_configs
                .into_iter()
                .map(|config| ChainConfig {
                    chain_code: config.chain_code,
                    chain_address_type: config.chain_address_type,
                    normal_address: IndexAndAddress {
                        index: config.normal_idx,
                        address: config.normal_address,
                    },
                    risk_address: IndexAndAddress {
                        index: config.risk_idx,
                        address: config.risk_address,
                    },
                })
                .collect();

            return Ok(Strategy {
                uid: local_strategy.uid,
                threshold: local_strategy.threshold as u32,
                chain_configs,
            });
        }

        // 2. 本地没有则从后端查询
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api().clone();
        let backend_resp = backend_api.query_withdrawal_strategy(uid).await?;

        // 3. 将后端结果保存到本地数据库
        self.save_withdraw_strategy_from_backend(uid, &backend_resp).await?;

        // 4. 转换为 Strategy 类型并返回
        let chain_configs = backend_resp
            .chain_configs
            .into_iter()
            .map(|config| ChainConfig {
                chain_code: config.chain_code,
                chain_address_type: config.chain_address_type,
                normal_address: IndexAndAddress {
                    index: config.normal_address.index,
                    address: config.normal_address.address,
                },
                risk_address: IndexAndAddress {
                    index: config.risk_address.index,
                    address: config.risk_address.address,
                },
            })
            .collect();

        Ok(Strategy { uid: uid.to_string(), threshold: backend_resp.threshold, chain_configs })
    }

    async fn save_withdraw_strategy_from_backend(
        &self,
        uid: &str,
        backend_resp: &wallet_transport_backend::response_vo::api_wallet::strategy::WithdrawStrategyResp,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 保存主策略
        let strategy_entity =
            wallet_database::entities::api_withdraw_strategy::ApiWithdrawStrategyEntity {
                id: 0, // 自增ID，插入时会自动生成
                uid: uid.to_string(),
                threshold: backend_resp.threshold as i32,
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

        // 使用upsert保存策略
        ApiWithdrawStrategyRepo::upsert(&pool, strategy_entity).await?;

        // 2. 获取刚插入的策略ID
        let strategy = ApiWithdrawStrategyRepo::get_by_uid(&pool, uid).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                        "保存提现策略失败".to_owned(),
                    ),
                ),
            ),
        )?;

        // 3. 保存链配置
        for config in &backend_resp.chain_configs {
            let chain_config_entity = wallet_database::entities::api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigEntity {
                id: 0, // 自增ID
                strategy_id: strategy.id,
                chain_code: config.chain_code.clone(),
                chain_address_type: config.chain_address_type.clone(),
                normal_idx: config.normal_address.index,
                normal_address: config.normal_address.address.clone(),
                risk_idx: config.risk_address.index,
                risk_address: config.risk_address.address.clone(),
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

            ApiWithdrawStrategyChainConfigRepo::upsert(&pool, chain_config_entity).await?;
        }

        Ok(())
    }

    pub async fn save_local_withdraw_strategy(
        &self,
        uid: &str,
        strategy: &wallet_transport_backend::request::api_wallet::strategy::Strategy,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1. 保存主策略
        let strategy_entity =
            wallet_database::entities::api_withdraw_strategy::ApiWithdrawStrategyEntity {
                id: 0, // 自增ID
                uid: uid.to_string(),
                threshold: strategy.threshold as i32,
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

        // 使用upsert保存策略
        ApiWithdrawStrategyRepo::upsert(&pool, strategy_entity).await?;

        // 2. 获取刚插入的策略ID
        let saved_strategy = ApiWithdrawStrategyRepo::get_by_uid(&pool, uid).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                        "保存提现策略失败".to_owned(),
                    ),
                ),
            ),
        )?;

        // 3. 先删除旧的链配置
        ApiWithdrawStrategyChainConfigRepo::delete_by_strategy_id(&pool, saved_strategy.id).await?;

        // 4. 保存新的链配置
        for config in &strategy.chain_configs {
            let chain_config_entity = wallet_database::entities::api_withdraw_strategy_chain_config::ApiWithdrawStrategyChainConfigEntity {
                id: 0, // 自增ID
                strategy_id: saved_strategy.id,
                chain_code: config.chain_code.clone(),
                chain_address_type: config.chain_address_type.clone(),
                normal_idx: config.normal_address.index,
                normal_address: config.normal_address.address.clone(),
                risk_idx: config.risk_address.index,
                risk_address: config.risk_address.address.clone(),
                created_at: sqlx::types::chrono::Utc::now(),
                updated_at: None,
            };

            ApiWithdrawStrategyChainConfigRepo::upsert(&pool, chain_config_entity).await?;
        }

        Ok(())
    }
}
