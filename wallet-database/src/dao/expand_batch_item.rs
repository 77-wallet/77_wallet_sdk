use sqlx::{Executor, Sqlite};

use crate::{
    entities::expand_batch_item::{
        CreateExpandBatchItemEntity, ExpandBatchItemEntity, ExpandItemStatus,
    },
    sql_utils::{SqlExecutableReturn as _, query_builder::DynamicQueryBuilder},
};

const INSERT_CHUNK: usize = 150; // 每批 insert 行数
const IN_CHUNK: usize = 900; // 每批 IN 参数数量（< 999）

pub struct ExpandBatchItemDao {}

impl ExpandBatchItemDao {
    /// 批量创建扩容项
    pub async fn batch_create<'a, E>(
        exec: E,
        items: Vec<CreateExpandBatchItemEntity>,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        if items.is_empty() {
            return Ok(());
        }

        for chunk in items.chunks(INSERT_CHUNK) {
            let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
                "INSERT INTO expand_batch_item
             (batch_id, uid, chain_code, input_index, status, created_at, updated_at)",
            );

            qb.push_values(chunk, |mut b, item| {
                b.push_bind(&item.batch_id)
                    .push_bind(&item.uid)
                    .push_bind(&item.chain_code)
                    .push_bind(item.input_index)
                    .push_bind(ExpandItemStatus::Pending)
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ','now')")
                    .push("strftime('%Y-%m-%dT%H:%M:%SZ','now')");
            });

            qb.push(" ON CONFLICT (batch_id, input_index) DO NOTHING");

            qb.build().execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;
        }

        Ok(())
    }

    /// 新增 list_status_by_indices
    pub async fn list_status_by_indices<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        input_indices: &[i32],
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        DynamicQueryBuilder::new("SELECT * FROM expand_batch_item")
            .and_where_eq("uid", uid)
            .and_where_eq("chain_code", chain_code)
            .and_where_in("input_index", input_indices)
            .fetch_all(exec)
            .await
    }

    pub async fn mark_item_status_by_batch<'a, E>(
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

    pub async fn mark_items_status_by_batch_from<'a, E>(
        exec: E,
        batch_id: &str,
        input_indices: &[i32],
        from: ExpandItemStatus,
        to: ExpandItemStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        if input_indices.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        for chunk in input_indices.chunks(IN_CHUNK) {
            let mut qb =
                sqlx::QueryBuilder::<Sqlite>::new("UPDATE expand_batch_item SET status = ");
            qb.push_bind(&to);
            qb.push(", updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ");
            qb.push("WHERE batch_id = ");
            qb.push_bind(batch_id);
            qb.push(" AND status = ");
            qb.push_bind(&from);
            qb.push(" AND input_index IN (");

            let mut sep = qb.separated(", ");
            for i in chunk {
                sep.push_bind(*i);
            }
            qb.push(")");

            let res =
                qb.build().execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

            total += res.rows_affected();
        }

        Ok(total)
    }

    pub async fn mark_items_status_by_owner_from<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
        input_indices: &[i32],
        from: &[ExpandItemStatus],
        to: ExpandItemStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        if input_indices.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        for chunk in input_indices.chunks(IN_CHUNK) {
            let mut qb =
                sqlx::QueryBuilder::<Sqlite>::new("UPDATE expand_batch_item SET status = ");
            qb.push_bind(&to);
            qb.push(", updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') ");
            qb.push("WHERE uid = ");
            qb.push_bind(uid);
            qb.push(" AND chain_code = ");
            qb.push_bind(chain_code);

            if !from.is_empty() {
                qb.push(" AND status IN (");
                let mut sep = qb.separated(", ");
                for s in from {
                    sep.push_bind(s);
                }
                qb.push(")");
            }

            qb.push(" AND input_index IN (");
            let mut sep = qb.separated(", ");
            for i in chunk {
                sep.push_bind(*i);
            }
            qb.push(")");

            let res =
                qb.build().execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

            total += res.rows_affected();
        }

        Ok(total)
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
          AND status IN (?)
    "#;

        sqlx::query_scalar(sql)
            .bind(uid)
            .bind(chain_code)
            .bind(ExpandItemStatus::Creating)
            // .bind(ExpandItemStatus::Initing)
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

    /// 获取当前 uid + chain 下，所有「曾被占用过」的 input_index
    ///
    /// ⚠️ 这是一个“永久占用集合”
    ///
    /// 一旦某个 index 出现在 expand_batch_item 中，
    /// 无论状态如何（Pending / Failed / Done），
    /// 都不得再次被分配。
    ///
    /// 用于：
    /// - index 分配
    /// - 扩容恢复（避免 index 重复）
    pub async fn get_all_occupied_indices<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
    ) -> Result<Vec<i32>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            SELECT DISTINCT input_index
            FROM expand_batch_item
            WHERE uid = ? AND chain_code = ?
            "#;

        let rows = sqlx::query_as::<_, (i32,)>(sql)
            .bind(uid)
            .bind(chain)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        let indices: Vec<i32> = rows.into_iter().map(|(i,)| i).collect();

        Ok(indices)
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
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        if indices.is_empty() {
            return Ok(Vec::new());
        }

        use std::collections::HashMap;
        let mut acc: HashMap<String, i64> = HashMap::new();

        for chunk in indices.chunks(IN_CHUNK) {
            let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
                "SELECT batch_id, COUNT(*) as cnt
             FROM expand_batch_item
             WHERE uid = ",
            );

            qb.push_bind(uid);
            qb.push(" AND chain_code = ");
            qb.push_bind(chain_code);
            qb.push(" AND input_index IN (");

            let mut sep = qb.separated(", ");
            for idx in chunk {
                sep.push_bind(*idx);
            }
            qb.push(") GROUP BY batch_id");

            let rows = qb
                .build_query_as::<(String, i64)>()
                .fetch_all(exec)
                .await
                .map_err(|e| crate::Error::Database(e.into()))?;

            for (bid, cnt) in rows {
                *acc.entry(bid).or_insert(0) += cnt;
            }
        }

        Ok(acc.into_iter().collect())
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

    pub async fn fetch_pending<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
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

        let items = sqlx::query_as::<Sqlite, ExpandBatchItemEntity>(sql)
            .bind(uid)
            .bind(chain_code)
            .bind(ExpandItemStatus::Pending)
            .bind(limit)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(items)
    }

    pub async fn fetch_retryable<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
        limit: i64,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
                SELECT * FROM expand_batch_item
        WHERE uid = ?
        AND chain_code = ?
        AND (
                status = ?
            OR (
                    status = ?
                AND updated_at < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-30 minutes')
                )
        )
        ORDER BY batch_id, input_index
        LIMIT ?
    "#;

        let items = sqlx::query_as::<Sqlite, ExpandBatchItemEntity>(sql)
            .bind(uid)
            .bind(chain)
            .bind(ExpandItemStatus::Pending)
            .bind(ExpandItemStatus::Failed)
            .bind(limit)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(items)
    }

    pub async fn mark_failed_and_inc_retry<'a, E>(
        exec: E,
        uid: &str,
        chain: &str,
        indices: &[i32],
        phase: ExpandItemStatus,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        if indices.is_empty() {
            return Ok(0);
        }

        let mut total = 0;

        for chunk in indices.chunks(IN_CHUNK) {
            let mut qb = sqlx::QueryBuilder::<Sqlite>::new(
                "UPDATE expand_batch_item
             SET status = ?, retry_count = retry_count + 1,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
             WHERE uid = ? AND chain_code = ? AND status = ? AND input_index IN (",
            );

            qb.push_bind(ExpandItemStatus::Failed);
            qb.push_bind(uid);
            qb.push_bind(chain);
            qb.push_bind(&phase);

            let mut sep = qb.separated(", ");
            for i in chunk {
                sep.push_bind(*i);
            }
            qb.push(")");

            let res =
                qb.build().execute(exec).await.map_err(|e| crate::Error::Database(e.into()))?;

            total += res.rows_affected();
        }

        Ok(total)
    }

    /// 将所有未完成的 item 重置为 Pending（用于 recover）
    pub async fn reset_unfinished_to_pending<'a, E>(
        exec: E,
        uid: &str,
        chain_code: &str,
    ) -> Result<u64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
        UPDATE expand_batch_item
        SET status = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE uid = ? AND chain_code = ?
          AND status != ?
    "#;

        let res = sqlx::query(sql)
            .bind(ExpandItemStatus::Pending)
            .bind(uid)
            .bind(chain_code)
            .bind(ExpandItemStatus::Done)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.rows_affected())
    }

    /// 统计特定状态的扩容项数量
    pub async fn count_by_status<'a, E>(
        exec: E,
        status: ExpandItemStatus,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM expand_batch_item WHERE status = ?";

        let count = sqlx::query_scalar(sql)
            .bind(status)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(count)
    }

    /// 统计所有扩容项数量
    pub async fn count_all<'a, E>(
        exec: E,
    ) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM expand_batch_item";

        let count = sqlx::query_scalar(sql)
            .fetch_one(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(count)
    }

    /// 获取所有扩容项
    pub async fn get_all<'a, E>(
        exec: E,
    ) -> Result<Vec<ExpandBatchItemEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM expand_batch_item";

        sqlx::query_as::<sqlx::Sqlite, ExpandBatchItemEntity>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }
}
