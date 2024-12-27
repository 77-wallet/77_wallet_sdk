pub struct CahcheDomain;

impl CahcheDomain {
    pub async fn get<T: serde::de::DeserializeOwned>(
        key: &str,
    ) -> Result<Option<T>, crate::ServiceError> {
        let cache = crate::Context::get_global_cache()?;

        let res = cache
            .get(key)
            .await
            .filter(|entry| !entry.is_expired())
            .map(|entry| wallet_utils::serde_func::serde_from_value(entry.data))
            .transpose();

        Ok(res?)
    }

    pub async fn set<T: serde::Serialize>(
        key: &str,
        value: T,
        expiration: u64,
    ) -> Result<(), crate::ServiceError> {
        let cache = crate::Context::get_global_cache()?;

        cache.set_ex(key, value, expiration).await
    }

    pub async fn del_key(key: &str) -> Result<(), crate::ServiceError> {
        let cache = crate::Context::get_global_cache()?;
        cache.delete(key).await?;
        Ok(())
    }
}
