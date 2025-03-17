use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct SharedCache {
    inner: RwLock<CacheMap>,
}
impl SharedCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(CacheMap::new()),
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

        let mut lock = self.inner.write().await;
        lock.set(key.to_string(), entry);

        Ok(())
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        let lock = self.inner.read().await;

        lock.get(key).cloned()
    }
    #[allow(dead_code)]
    pub async fn delete(&self, key: &str) -> Result<(), crate::ServiceError> {
        let mut lock = self.inner.write().await;
        lock.delete(key);

        Ok(())
    }

    pub async fn get_not_expriation(&self, key: &str) -> Option<CacheEntry> {
        let lock = self.inner.read().await;

        let result = lock.get(key).cloned();
        drop(lock);

        if let Some(result) = result {
            if result.is_expired() {
                {
                    let mut lock = self.inner.write().await;
                    lock.delete(key);
                }
                None
            } else {
                Some(result)
            }
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct CacheMap(HashMap<String, CacheEntry>);
impl CacheMap {
    fn new() -> Self {
        CacheMap(HashMap::new())
    }

    fn set(&mut self, key: String, entry: CacheEntry) {
        self.0.insert(key, entry);
    }

    fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.0.get(key)
    }

    fn delete(&mut self, key: &str) {
        self.0.remove(key);
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
        let instant =
            expiration.map(|i| std::time::Instant::now() + std::time::Duration::from_secs(i));

        Ok(Self { data, instant })
    }

    pub fn is_expired(&self) -> bool {
        self.instant
            .is_some_and(|instant| instant <= std::time::Instant::now())
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
