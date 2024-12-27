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

    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        let lock = self.inner.read().await;

        lock.get(key).cloned()
    }

    pub async fn delete(&self, key: &str) -> Result<(), crate::ServiceError> {
        let mut lock = self.inner.write().await;
        lock.delete(key);

        Ok(())
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
        let instant = expiration
            .and_then(|i| Some(std::time::Instant::now() + std::time::Duration::from_secs(i)));

        Ok(Self { data, instant })
    }

    pub fn is_expired(&self) -> bool {
        self.instant
            .map_or(false, |instant| instant <= std::time::Instant::now())
    }
}
