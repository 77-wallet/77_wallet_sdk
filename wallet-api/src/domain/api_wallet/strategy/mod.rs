use wallet_database::repositories::api_wallet::collect_strategy::ApiCollectStrategyRepo;

pub(crate) struct StrategyDomain {}

impl StrategyDomain {
    async fn query_local_collect_strategy(
        &self,
        uid: &str,
    ) -> Result<
        wallet_transport_backend::request::api_wallet::strategy::Strategy,
        crate::error::service::ServiceError,
    > {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let collect_strategy = ApiCollectStrategyRepo::get_by_uid(&pool, uid).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                        "本地策略不存在".to_owned(),
                    ),
                ),
            ),
        )?;

        // 这里需要将 ApiCollectStrategyEntity 转换为 Strategy 类型
        // 由于结构差异较大，这里暂时返回一个空的 Strategy
        // 实际应用中需要根据业务逻辑进行转换
        Ok(wallet_transport_backend::request::api_wallet::strategy::Strategy {
            uid: collect_strategy.uid,
            threshold: collect_strategy.min_value.parse().unwrap_or(0),
            chain_configs: Vec::new(),
        })
    }

    async fn save_local_collect_strategy(
        &self,
        uid: &str,
        strategy: &wallet_transport_backend::request::api_wallet::strategy::Strategy,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 这里需要将 Strategy 类型转换为 ApiCollectStrategyEntity 类型
        // 由于结构差异较大，这里暂时不实现保存逻辑
        // 实际应用中需要根据业务逻辑进行转换
        Ok(())
    }
}
