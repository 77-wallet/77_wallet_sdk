use sqlx::Pool;
use crate::{dao::api_window::ApiWindowDao, DbPool};

pub struct ApiWindowRepo;

impl ApiWindowRepo {
    pub async fn get_api_offset(pool: &DbPool, id: i64) -> Result<i64, crate::Error> {
        ApiWindowDao::get_api_offset(pool.as_ref(), id).await
    }

    pub async fn upsert_api_offset(pool: &DbPool,   id: i64, offset: i64) -> Result<(), crate::Error>
    {
        ApiWindowDao::upsert_api_offset(pool.as_ref(), id, offset).await
    }
}
