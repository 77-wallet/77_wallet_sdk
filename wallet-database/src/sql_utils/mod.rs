pub mod delete_builder;
pub mod query_builder;
pub mod update_builder;

use async_trait::async_trait;
use sqlx::{Executor, Sqlite};

#[derive(Debug, Clone)]
pub enum SqlArg {
    Str(String),
    Int(i64),
    Bool(bool),
    // Option<T> 扩展后可加：OptionalStr(Option<String>) 等
}

pub trait SqlBuilder {
    fn build(&self) -> (String, Vec<SqlArg>);
}

#[async_trait]
pub trait SqlExecutableNoReturn: SqlBuilder + Sync {
    async fn execute<'e, E>(&self, executor: E) -> Result<(), crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, args) = self.build();
        let query = bind_all_execute(sqlx::query(&sql), &args);
        query
            .execute(executor)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}

#[async_trait]
pub trait SqlExecutableReturn<T>: Sync + SqlBuilder
where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static,
{
    // fn build(&self) -> (String, Vec<SqlArg>);

    async fn fetch_all<'e, E>(&self, executor: E) -> Result<Vec<T>, crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, args) = self.build();
        tracing::info!("query: {}", sql);
        let query = bind_all_args(sqlx::query_as::<_, T>(&sql), &args);
        query
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    async fn fetch_optional<'e, E>(&self, executor: E) -> Result<Option<T>, crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        {
            let (sql, args) = self.build();
            tracing::info!("query: {}", sql);
            let query = bind_all_args(sqlx::query_as::<_, T>(&sql), &args);
            query
                .fetch_optional(executor)
                .await
                .map_err(|e| crate::Error::Database(e.into()))
        }
    }

    async fn execute<'e, E>(&self, executor: E) -> Result<(), crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, args) = self.build();
        tracing::info!("execute: {}", sql);
        let query = bind_all_execute(sqlx::query(&sql), &args);
        query
            .execute(executor)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }
}

pub fn bind_all_execute<'q>(
    query: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    args: &'q [SqlArg],
) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    args.iter().fold(query, |query, arg| match arg {
        SqlArg::Str(s) => query.bind(s),
        SqlArg::Int(i) => query.bind(i),
        SqlArg::Bool(b) => query.bind(b),
    })
}

pub fn bind_all_args<'q, T>(
    query: sqlx::query::QueryAs<'q, Sqlite, T, sqlx::sqlite::SqliteArguments<'q>>,
    args: &'q [SqlArg],
) -> sqlx::query::QueryAs<'q, Sqlite, T, sqlx::sqlite::SqliteArguments<'q>>
where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
{
    args.iter().fold(query, |query, arg| match arg {
        SqlArg::Str(s) => query.bind(s),
        SqlArg::Int(i) => query.bind(i),
        SqlArg::Bool(b) => query.bind(b),
    })
}
