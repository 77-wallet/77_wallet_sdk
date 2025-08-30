use sqlx::{Executor, Sqlite};

pub(crate) struct ApiWindowDao;

impl ApiWindowDao {
    pub async fn get_api_offset<'c, E>(executor: E, id: i64) -> Result<i64, crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"SELECT offset FROM api_window where id = $1"#;
        let result = sqlx::query_scalar::<_, i64>(sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        if result.is_none() {
            return Ok(0);
        }
        Ok(result.unwrap())
    }

    pub async fn upsert_api_offset<'c, E>(
        executor: E,
        id: i64,
        offset: i64,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let sql = r#"
            Insert into api_window
                (id,offset,created_at,updated_at)
            values
                ($1, $2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            on conflict (id)
            do update set
                offset = $2,
                updated_at = excluded.updated_at
            returning offset
        "#;

        let offset = sqlx::query_scalar::<_, i32>(sql)
            .bind(id)
            .bind(offset)
            .fetch_one(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }
}
