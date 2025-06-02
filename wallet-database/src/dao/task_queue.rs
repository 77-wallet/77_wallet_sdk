use sqlx::{Executor, Sqlite};

use crate::entities::task_queue::{CreateTaskQueueEntity, TaskQueueEntity};

impl TaskQueueEntity {
    pub async fn upsert_multi_task<'a, E>(
        exec: E,
        reqs: &[CreateTaskQueueEntity],
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "insert into task_queue (id, task_name, request_body, type, status, created_at, updated_at) ",
        );
        query_builder.push_values(reqs, |mut b, req| {
            b.push_bind(req.id.clone())
                .push_bind(req.task_name.clone())
                .push_bind(req.request_body.clone().unwrap_or_default())
                .push_bind(req.r#type)
                .push_bind(req.status)
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')");
        });

        query_builder.push(
            " on conflict (id) do update set updated_at = excluded.updated_at
            RETURNING *",
        );

        let query = query_builder.build_query_as::<TaskQueueEntity>();

        query
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn upsert<'a, E>(exec: E, req: CreateTaskQueueEntity) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "INSERT INTO task_queue (id, task_name, request_body, type, status, created_at, updated_at)
            VALUES
            (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT (id) DO UPDATE SET updated_at = excluded.updated_at";
        sqlx::query(sql)
            .bind(req.id)
            .bind(req.task_name)
            .bind(req.request_body)
            .bind(req.r#type)
            .bind(req.status)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn update_status<'a, E>(exec: E, id: &str, status: u8) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE task_queue SET status = ?,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') 
                        WHERE id = ?";
        sqlx::query(sql)
            .bind(status)
            .bind(id)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn increase_retry_times<'a, E>(exec: E, id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE task_queue SET retry_times = retry_times + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                        WHERE id = ?";
        sqlx::query(sql)
            .bind(id)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete<'a, E>(exec: E, id: &str) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM task_queue WHERE id = ?";

        sqlx::query(sql)
            .bind(id)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_old<'a, E>(exec: E, day: u16) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // 删除超过7天的数据
        let sql = "DELETE FROM task_queue WHERE julianday('now') - julianday(created_at) > ?";

        sqlx::query(sql)
            .bind(day)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_oldest_by_status_when_exceeded<'a, E>(
        exec: &E,
        max_size: u32,
        target_status: u8,
    ) -> Result<(), crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        // 获取当前总记录数
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_queue")
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        let max_size = max_size as i64;
        if count <= max_size {
            return Ok(());
        }

        let over_count = count - max_size;

        // 删除指定状态的最早记录
        let sql = "
            DELETE FROM task_queue
            WHERE id IN (
                SELECT id 
                FROM task_queue 
                WHERE status = ? 
                ORDER BY created_at ASC 
                LIMIT ?
            )";

        sqlx::query(sql)
            .bind(target_status)
            .bind(over_count)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_all<'a, E>(exec: E, typ: Option<u8>) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "DELETE FROM task_queue".to_string();
        let mut conditions = Vec::new();
        if typ.is_some() {
            conditions.push("type = ?".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query(&sql);

        if let Some(typ) = typ {
            query = query.bind(typ);
        }

        query
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_task_queue<'a, E>(exec: E, id: &str) -> Result<Option<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM task_queue WHERE id = ?";

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_tasks_with_request_body<'a, E>(
        exec: E,
        keyword: &str,
    ) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM task_queue WHERE request_body LIKE ?";

        let pattern = format!("%{}%", keyword);

        sqlx::query_as::<sqlx::Sqlite, Self>(sql)
            .bind(pattern)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn delete_tasks_with_request_body_like<'a, E>(
        exec: E,
        keyword: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "DELETE FROM task_queue WHERE request_body LIKE ?";
        let pattern = format!("%{}%", keyword);
        sqlx::query(sql)
            .bind(pattern)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn list<'a, E>(exec: E, status: Option<u8>) -> Result<Vec<Self>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let mut sql = "SELECT * FROM task_queue".to_string();
        let mut conditions = Vec::new();
        if status.is_some() {
            conditions.push("status = ?".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = sqlx::query_as::<sqlx::Sqlite, Self>(&sql);

        if let Some(status) = status {
            query = query.bind(status);
        }

        query
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn has_unfinished_task<'a, E>(exec: E) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT EXISTS(SELECT 1 FROM task_queue WHERE status != 2)";
        let exists: i64 = sqlx::query_scalar(sql)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(exists == 1)
    }
}
