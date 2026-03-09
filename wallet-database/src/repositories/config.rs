use crate::{CoreDbPool, dao::config::ConfigDao, entities::config::ConfigEntity};

pub struct ConfigRepo;

impl ConfigRepo {
    pub async fn upsert(
        key: &str,
        value: &str,
        types: Option<i8>,
        pool: &CoreDbPool,
    ) -> Result<ConfigEntity, crate::Error> {
        ConfigDao::upsert(key, value, types, pool.as_ref()).await
    }

    pub async fn list_v2(pool: &CoreDbPool) -> Result<Vec<ConfigEntity>, crate::Error> {
        ConfigDao::list_v2(pool.as_ref()).await
    }

    pub async fn find_by_key(
        key: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<ConfigEntity>, crate::Error> {
        ConfigDao::find_by_key(key, pool.as_ref()).await
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigRepo;

    fn make_temp_dir(prefix: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().to_string()
    }

    #[tokio::test]
    async fn config_repo_upsert_and_find_by_key_success() {
        let dir = make_temp_dir("wallet_db_config_repo_happy");
        let ctx = crate::SqliteContext::new(&dir, Some("data.db")).await.unwrap();
        let pool = ctx.into_core_db_pool().unwrap();

        let saved = ConfigRepo::upsert("k1", "v1", Some(0), &pool).await.unwrap();
        assert_eq!(saved.key, "k1");
        assert_eq!(saved.value, "v1");

        let found = ConfigRepo::find_by_key("k1", &pool).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.key, "k1");
        assert_eq!(found.value, "v1");
    }

    #[tokio::test]
    async fn config_repo_find_by_key_missing_returns_none() {
        let dir = make_temp_dir("wallet_db_config_repo_missing");
        let ctx = crate::SqliteContext::new(&dir, Some("data.db")).await.unwrap();
        let pool = ctx.into_core_db_pool().unwrap();

        let found = ConfigRepo::find_by_key("missing_key", &pool).await.unwrap();
        assert!(found.is_none());
    }
}
