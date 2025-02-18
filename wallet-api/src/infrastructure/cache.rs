#[derive(Debug)]
pub struct SharedCache {
    inner: dashmap::DashMap<String, CacheEntry>,
}
impl SharedCache {
    pub fn new() -> Self {
        Self {
            inner: dashmap::DashMap::new(),
        }
    }

    // expiration unit is secs
    pub async fn set_ex<T: serde::Serialize>(
        &self,
        key: &str,
        value: T,
        expiration: u64,
    ) -> Result<(), crate::ServiceError> {
        let entry = CacheEntry::new(value, Some(expiration))?;

        self.inner.insert(key.to_owned(), entry);

        Ok(())
    }

    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        self.inner.get(key).map(|entry| entry.value().clone())
    }

    pub async fn delete(&self, key: &str) -> Result<(), crate::ServiceError> {
        self.inner.remove(key);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub data: serde_json::Value,
    instant: Option<std::time::Instant>,
}

impl CacheEntry {
    fn new<T: serde::Serialize>(
        data: T,
        expiration: Option<u64>,
    ) -> Result<Self, crate::ServiceError> {
        let data = wallet_utils::serde_func::serde_to_value(data)?;
        let instant = expiration
            .and_then(|i| Some(std::time::Instant::now() + std::time::Duration::from_secs(i)));

        Ok(Self { data, instant })
    }

    pub fn is_expired(&self) -> bool {
        self.instant
            .map_or(false, |instant| instant <= std::time::Instant::now())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_cache() {
        let cache = SharedCache::new();

        let _rs = cache.set_ex("hello", "world", 10).await;
        let rs = cache.get("hello").await;

        println!("{:?}", rs);
    }

    #[tokio::test]
    async fn test_cached() {
        let cache = SharedCache::new();

        let _rs = cache.set_ex("hello", "world", 10).await;

        let rs = cache.get("hello").await;
        println!("{:?}", rs);

        println!("睡眠5秒");
        sleep(std::time::Duration::from_secs(5)).await;

        let rs = cache.get("hello").await;
        println!(
            "再次获取 {:?},是否过期 {}",
            rs.clone(),
            rs.unwrap().is_expired()
        );

        println!("睡眠6秒");
        sleep(std::time::Duration::from_secs(5)).await;

        let rs = cache.get("hello").await;
        println!(
            "再次获取 {:?},是否过期 {}",
            rs.clone(),
            rs.unwrap().is_expired()
        );
    }
}
