use sqlx::{Executor, Sqlite};

use crate::entities::expand_batch_item::{
    CreateExpandBatchItemEntity, ExpandBatchItemEntity, ExpandItemStatus,
};

pub struct ExpandBatchItemDao {}

impl ExpandBatchItemDao {
    /// 批量创建扩容项
    pub async fn batch_create<'a, E>(
        exec: E,
        items: Vec<CreateExpandBatchItemEntity>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if items.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::<Sqlite>::new(
            "INSERT INTO expand_batch_item 
            (batch_id, uid, chain_code, input_index, status, created_at, updated_at)",
        );

        query_builder.push_values(items, |mut b, item| {
            b.push_bind(item.batch_id.clone())
                .push_bind(item.uid.clone())
                .push_bind(item.chain_code.clone())
                .push_bind(item.input_index)
                .push_bind(ExpandItemStatus::Pending)
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')") // created_at
                .push("strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"); // updated_at
        });

        query_builder.push(" ON CONFLICT (batch_id, input_index) DO NOTHING");

        let query = query_builder.build();
        query.execute(exec).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_item_status<'a, E>(
        exec: E,
        batch_id: &str,
        input_index: i32,
        status: ExpandItemStatus,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch_item 
        SET 
            status = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE batch_id = ? AND input_index = ?
    "#;

        sqlx::query(sql)
            .bind(status)
            .bind(batch_id)
            .bind(input_index)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn mark_items_status_from<'a, E>(
        exec: E,
        batch_id: &str,
        input_indices: &[i32],
        from: ExpandItemStatus,
        to: ExpandItemStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if input_indices.is_empty() {
            return Ok(0);
        }

        let mut qb = sqlx::QueryBuilder::<Sqlite>::new("UPDATE expand_batch_item SET status = ");
        qb.push_bind(to);
        qb.push(", updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ");
        qb.push("WHERE batch_id = ");
        qb.push_bind(batch_id);
        qb.push(" AND status = ");
        qb.push_bind(from);
        qb.push(" AND input_index IN (");

        let mut sep = qb.separated(", ");
        for i in input_indices {
            sep.push_bind(*i);
        }
        qb.push(")");

        let res = qb.build().execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res.rows_affected())
    }

    pub async fn fetch_by_status<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        status: ExpandItemStatus,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch_item
        WHERE uid = ? AND chain_code = ? AND status = ?
        ORDER BY batch_id, input_index
        LIMIT ?
    "#;

        sqlx::query_as(sql)
            .bind(uid)
            .bind(chain_code)
            .bind(status)
            .bind(limit)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn count_inflight<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT COUNT(*) FROM expand_batch_item
        WHERE uid = ? AND chain_code = ?
          AND status IN (?, ?)
    "#;

        sqlx::query_scalar(sql)
            .bind(uid)
            .bind(chain_code)
            .bind(ExpandItemStatus::Creating)
            .bind(ExpandItemStatus::Initing)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 获取批次的所有扩容项
    pub async fn get_items_by_batch_id<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT * FROM expand_batch_item 
            WHERE batch_id = ?
            ORDER BY input_index
        "#;

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchItemEntity>(sql)
            .bind(batch_id)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    /// 检查某个批次的所有扩容项是否都已完成
    pub async fn is_batch_all_done<'a, E>(exec: E, batch_id: &str) -> Result<bool, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT 
                COUNT(*) > 0
                AND COUNT(*) = SUM(CASE WHEN status = ? THEN 1 ELSE 0 END)
            FROM expand_batch_item
            WHERE batch_id = ?
        "#;

        let is_done: Option<bool> = sqlx::query_scalar(sql)
            .bind(ExpandItemStatus::Done)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(is_done.unwrap_or(false))
    }

    /// 获取批次的完成进度
    pub async fn get_batch_progress<'a, E>(
        exec: E,
        batch_id: &str,
    ) -> Result<(i32, i32), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT 
                COUNT(*) as total_count,
                SUM(CASE WHEN status = ? THEN 1 ELSE 0 END) as finished_count
            FROM expand_batch_item 
            WHERE batch_id = ?
        "#;

        let result: Option<(i32, i32)> = sqlx::query_as(sql)
            .bind(ExpandItemStatus::Done)
            .bind(batch_id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(result.unwrap_or((0, 0)))
    }

    /// 根据链代码和输入索引查找受影响的批次，并返回每个批次命中的数量
    pub async fn find_batches_by_indices<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        indices: &[i32],
    ) -> Result<Vec<(String, i64)>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        if indices.is_empty() {
            return Ok(Vec::new());
        }

        let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
            "SELECT batch_id, COUNT(*) as cnt \
         FROM expand_batch_item \
         WHERE uid = ",
        );

        qb.push_bind(uid);
        qb.push(" AND chain_code = ");
        qb.push_bind(chain_code);
        qb.push(" AND input_index IN (");

        let mut separated = qb.separated(", ");
        for idx in indices {
            separated.push_bind(*idx);
        }
        qb.push(") GROUP BY batch_id");

        let query = qb.build_query_as::<(String, i64)>();
        let rows = query.fetch_all(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rows)
    }

    pub async fn fetch_by_batch_and_status<'a, E>(
        exec: E,
        batch_id: &str,
        status: ExpandItemStatus,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        SELECT * FROM expand_batch_item
        WHERE batch_id = ? AND status = ?
        ORDER BY input_index
        "#;

        sqlx::query_as(sql)
            .bind(batch_id)
            .bind(status)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn fetch_and_mark_pending<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            UPDATE expand_batch_item
            SET status = ?
            WHERE rowid IN (
                SELECT rowid FROM expand_batch_item
                WHERE uid=? AND chain_code=? AND status=?
                ORDER BY batch_id, input_index
                LIMIT ?
            )
            RETURNING *;
        "#;

        let items = sqlx::query_as::<Sqlite, ExpandBatchItemEntity>(sql)
            .bind(ExpandItemStatus::Creating) // ✅ 先统一抢成 Creating
            .bind(uid)
            .bind(chain_code)
            .bind(ExpandItemStatus::Pending)
            .bind(limit)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(items)
    }
}
