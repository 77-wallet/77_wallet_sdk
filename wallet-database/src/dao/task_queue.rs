use sqlx::{Executor, Sqlite};

use crate::{
    entities::task_queue::{CreateTaskQueueEntity, TaskName, TaskQueueEntity},
    sql_utils::{
        SqlExecutableNoReturn, SqlExecutableReturn, delete_builder::DynamicDeleteBuilder,
        query_builder::DynamicQueryBuilder, update_builder::DynamicUpdateBuilder,
    },
};

pub struct TaskQueueDao {}
pub type CreateTaskQueueDao = CreateTaskQueueEntity;

impl TaskQueueDao {
    /// 批量插入task_queue记录
    pub async fn insert_batch<'a, E>(exec: E, tasks: &[TaskQueueEntity]) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if tasks.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "insert into task_queue (id, task_name, request_body, type, status, err_msg, remark, created_at, updated_at) ",
        );
        query_builder.push_values(tasks, |mut b, task| {
            b.push_bind(task.id.clone())
                .push_bind(task.task_name.clone())
                .push_bind(task.request_body.clone())
                .push_bind(task.r#type)
                .push_bind(task.status)
                .push_bind(task.err_msg.clone())
                .push_bind(task.remark.clone())
                .push_bind(task.created_at)
                .push_bind(task.updated_at);
        });

        let query = query_builder.build();
        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    /// 批量插入task_queue记录，忽略冲突
    pub async fn insert_batch_ignore_conflict<'a, E>(
        exec: E,
        tasks: &[TaskQueueEntity],
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if tasks.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "insert into task_queue (id, task_name, request_body, type, status, err_msg, remark, created_at, updated_at) ",
        );
        query_builder.push_values(tasks, |mut b, task| {
            b.push_bind(task.id.clone())
                .push_bind(task.task_name.clone())
                .push_bind(task.request_body.clone())
                .push_bind(task.r#type)
                .push_bind(task.status)
                .push_bind(task.err_msg.clone())
                .push_bind(task.remark.clone())
                .push_bind(task.created_at)
                .push_bind(task.updated_at);
        });
        query_builder.push(" ON CONFLICT (id) DO NOTHING");

        let query = query_builder.build();
        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    /// 检查表是否存在
    pub async fn table_exists<'a, E>(exec: E, table_name: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT name FROM sqlite_master WHERE type='table' AND name = ?";
        let result = sqlx::query_scalar::<_, String>(sql)
            .bind(table_name)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(result.is_some())
    }

    /// 单个CreateTaskQueueEntity的upsert
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

    /// 多个CreateTaskQueueEntity的upsert
    pub async fn upsert_multi_task<'a, E>(
        exec: E,
        reqs: &[CreateTaskQueueEntity],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
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

        query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))
    }

    /// 根据状态和类型查询任务
    pub async fn list<'a, E>(
        exec: E,
        status: Option<u8>,
        typ: Option<u8>,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let mut builder = DynamicQueryBuilder::new("SELECT * FROM task_queue");
        if let Some(status) = status {
            builder = builder.and_where_eq("status", status);
        }
        if let Some(typ) = typ {
            builder = builder.and_where_eq("type", typ);
        }
        builder.fetch_all(exec).await
    }

    /// 获取所有task_queue记录
    pub async fn get_all<'a, E>(exec: E) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT id, task_name, request_body, type, status, err_msg, remark, created_at, updated_at FROM task_queue";
        sqlx::query_as::<sqlx::Sqlite, TaskQueueEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 根据ID获取单个task_queue记录
    pub async fn get_task_queue<'a, E>(
        exec: E,
        id: &str,
    ) -> Result<Option<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let sql = "SELECT * FROM task_queue WHERE id = ?";
        sqlx::query_as::<sqlx::Sqlite, TaskQueueEntity>(sql)
            .bind(id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 更新task_queue状态
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

    /// 更新task_queue为失败状态
    pub async fn task_failed<'a, E>(
        exec: E,
        id: &str,
        status: u8,
        err_msg: &str,
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicUpdateBuilder::new("task_queue")
            .set("status", status)
            .set("err_msg", err_msg)
            .set_raw("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')")
            .and_where_eq("id", id)
            .returning("*")
            .fetch_all(exec)
            .await
    }

    /// 根据task_name和status获取单个任务
    pub async fn get_task_with_task_name<'a, E>(
        exec: E,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Option<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder = DynamicQueryBuilder::new("SELECT * FROM task_queue");
        builder
            .and_where_eq("task_name", task_name)
            .and_where_in("status", status)
            .fetch_optional(exec)
            .await
    }

    /// 根据task_name和status获取任务列表
    pub async fn list_tasks_with_task_name<'a, E>(
        exec: E,
        task_name: TaskName,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder = DynamicQueryBuilder::new("SELECT * FROM task_queue");
        builder
            .and_where_eq("task_name", task_name)
            .and_where_in("status", status)
            .fetch_all(exec)
            .await
    }

    /// 根据request_body关键词和status获取任务列表
    pub async fn get_tasks_with_request_body<'a, E>(
        exec: E,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder = DynamicQueryBuilder::new("SELECT * FROM task_queue");
        builder
            .and_where_like("request_body", keyword)
            .and_where_in("status", status)
            .fetch_all(exec)
            .await
    }

    /// 根据task_name、request_body关键词和status获取任务列表
    pub async fn get_tasks_with_request_body_and_task_name<'a, E>(
        exec: E,
        task_name: TaskName,
        keyword: &str,
        status: &[u8],
    ) -> Result<Vec<TaskQueueEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder = DynamicQueryBuilder::new("SELECT * FROM task_queue");
        builder
            .and_where_eq("task_name", task_name)
            .and_where_like("request_body", keyword)
            .and_where_in("status", status)
            .fetch_all(exec)
            .await
    }

    /// 更新任务备注
    pub async fn update_task_remark<'a, E>(
        exec: E,
        id: &str,
        remark: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder =
            DynamicUpdateBuilder::new("task_queue").set("remark", remark).and_where_eq("id", id);
        SqlExecutableNoReturn::execute(builder, exec).await
    }

    /// 增加重试次数
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

    /// 删除相关操作
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
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_queue")
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        let max_size = max_size as i64;
        if count <= max_size {
            return Ok(());
        }

        let over_count = count - max_size;
        let sql = "DELETE FROM task_queue WHERE id IN (SELECT id FROM task_queue WHERE status = ? ORDER BY created_at ASC LIMIT ?)";
        sqlx::query(sql)
            .bind(target_status)
            .bind(over_count)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 根据类型删除所有任务
    pub async fn delete_all<'a, E>(exec: E, typ: Option<u8>) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = "DELETE FROM task_queue".to_string();
        let mut conditions = Vec::new();
        if let Some(typ) = typ {
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

        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    /// 根据request_body关键词删除任务
    pub async fn delete_tasks_with_request_body_like<'a, E>(
        exec: E,
        keyword: &str,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + 'a,
    {
        let builder =
            DynamicDeleteBuilder::new("task_queue").and_where_like("request_body", keyword);
        SqlExecutableNoReturn::execute(builder, exec).await
    }

    /// 统计相关操作
    pub async fn count<'a, E>(exec: E) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM task_queue";
        sqlx::query_scalar(sql).fetch_one(exec).await.map_err(|e| crate::Error::Database(e.into()))
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

    pub async fn has_unfinished_task_by_type<'a, E>(exec: E, typ: u8) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT EXISTS(SELECT 1 FROM task_queue WHERE status != 2 AND type = ?)";

        let exists: i64 = sqlx::query_scalar(sql)
            .bind(typ)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(exists == 1)
    }

    /// 将task_queue表重命名为task_queue_legacy（冻结旧表）
    pub async fn freeze_table<'a, E>(exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "ALTER TABLE task_queue RENAME TO task_queue_legacy";
        sqlx::query(sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
