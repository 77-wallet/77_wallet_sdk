use serde::Serialize;
use sqlx::{sqlite::SqliteRow, Executor, FromRow, Sqlite};

// TODO 整个模块修改错误类型
#[derive(Debug, Serialize)]
pub struct Pagination<T: Serialize> {
    pub page: i64,
    pub page_size: i64,
    pub total_count: i64,
    pub data: Vec<T>,
}

impl<T> Pagination<T>
where
    T: for<'r> FromRow<'r, SqliteRow> + Unpin + Send + 'static + std::fmt::Debug + Serialize,
{
    pub fn init(page: i64, page_size: i64) -> Self {
        Self {
            page,
            page_size,
            total_count: 0,
            data: Vec::new(),
        }
    }

    pub async fn page_v1<'a, E>(mut self, exec: &E, sql: &str) -> Result<Self, crate::DatabaseError>
    where
        for<'c> &'c E: Executor<'c, Database = Sqlite>,
    {
        self.total_count = self.total_count_v1(exec, sql).await?;

        let sql = format!(
            "{} LIMIT {} OFFSET {}",
            sql,
            self.page_size,
            self.page * self.page_size
        );

        let data = sqlx::query_as::<_, T>(&sql).fetch_all(exec).await?;
        self.data = data;

        Ok(self)
    }

    pub async fn data<'a, E>(&self, sql: &str, exec: &E) -> Result<Vec<T>, crate::Error>
    where
        for<'c> &'c E: Executor<'c, Database = Sqlite>,
    {
        let res = sqlx::query_as::<_, T>(sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn group_count<'a, E>(&self, sql: &str, exec: &E) -> Result<i64, crate::Error>
    where
        for<'c> &'c E: Executor<'c, Database = Sqlite>,
    {
        let count = sqlx::query_scalar::<_, i64>(sql).fetch_one(exec).await;

        match count {
            Ok(count) => Ok(count),
            Err(_e) => Ok(0),
        }
    }

    pub async fn total_count_v1<'a, E>(
        &self,
        exec: &E,
        sql: &str,
    ) -> Result<i64, crate::DatabaseError>
    where
        for<'c> &'c E: Executor<'c, Database = Sqlite>,
    {
        let count_sql = "SELECT count(*) FROM";

        let start = sql.find("FROM").unwrap_or(0) + 4;
        let sql = format!("{} {}", count_sql, &sql[start..]);

        let count = sqlx::query_scalar::<_, i64>(&sql).fetch_one(exec).await;

        match count {
            Ok(count) => Ok(count),
            Err(_e) => Ok(0),
        }
    }

    // #[deprecated]
    // pub async fn total_count<'a, E>(&self, exec: &E, sql: &str) -> Result<i64, crate::Error>
    // where
    //     for<'c> &'c E: Executor<'c, Database = Sqlite>,
    // {
    //     let count_sql = "SELECT count(*) FROM";

    //     let start = sql.find("FROM").unwrap_or(0) + 4;
    //     let sql = format!("{} {}", count_sql, &sql[start..]);

    //     let count = sqlx::query_scalar::<_, i64>(&sql).fetch_one(exec).await;

    //     match count {
    //         Ok(count) => Ok(count),
    //         Err(_e) => Ok(0),
    //     }
    // }

    // #[deprecated]
    // pub async fn page<'a, E>(mut self, exec: &E, sql: &str) -> Result<Self, crate::Error>
    // where
    //     for<'c> &'c E: Executor<'c, Database = Sqlite>,
    // {
    //     self.total_count = self.total_count(exec, sql).await?;

    //     let sql = format!(
    //         "{} LIMIT {} OFFSET {}",
    //         sql,
    //         self.page_size,
    //         self.page * self.page_size
    //     );

    //     // tracing::info!("result sql = {}", sql);
    //     let res = sqlx::query_as::<_, T>(&sql)
    //         .fetch_all(exec)
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))?;

    //     // tracing::info!("result res = {:?}", res);
    //     self.data = res;
    //     Ok(self)
    // }
}
