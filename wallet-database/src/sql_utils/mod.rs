pub(crate) mod delete_builder;
pub(crate) mod query_builder;
pub(crate) mod update_builder;

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{Executor, Sqlite, sqlite::SqliteArguments};

pub type ArgFn<'q> = Arc<dyn Fn(&mut SqliteArguments<'q>) + Send + Sync + 'q>;

pub trait SqlQueryBuilder<'q> {
    fn build_sql(&self) -> (String, Vec<ArgFn<'q>>);
}

#[async_trait]
pub trait SqlExecutableNoReturn<'a>: SqlQueryBuilder<'a> {
    async fn execute<'e, E>(&self, executor: E) -> Result<(), crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, args_fn) = self.build_sql();
        let mut args = SqliteArguments::default();
        for f in args_fn {
            f(&mut args);
        }
        let query = sqlx::query_with(&sql, args);

        query.execute(executor).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }
}

#[async_trait]
pub trait SqlExecutableReturn<'a, T>: SqlQueryBuilder<'a>
where
    for<'r> T: sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'a,
{
    async fn fetch_all<'e, E>(&self, executor: E) -> Result<Vec<T>, crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, arg_fns) = self.build_sql();
        tracing::info!("query: {}", sql);

        let mut args = SqliteArguments::default();
        for f in arg_fns {
            f(&mut args);
        }
        let query = sqlx::query_as_with::<_, T, _>(&sql, args);

        query.fetch_all(executor).await.map_err(|e| crate::Error::Database(e.into()))
    }

    async fn fetch_optional<'e, E>(&self, executor: E) -> Result<Option<T>, crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        {
            let (sql, arg_fns) = self.build_sql();
            tracing::info!("query: {}", sql);

            let mut args = SqliteArguments::default();
            for f in arg_fns {
                f(&mut args);
            }
            let query = sqlx::query_as_with::<_, T, _>(&sql, args);

            query.fetch_optional(executor).await.map_err(|e| crate::Error::Database(e.into()))
        }
    }

    async fn fetch_one<'e, E>(&self, executor: E) -> Result<T, crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        {
            let (sql, arg_fns) = self.build_sql();
            tracing::info!("query: {}", sql);

            let mut args = SqliteArguments::default();
            for f in arg_fns {
                f(&mut args);
            }
            let query = sqlx::query_as_with::<_, T, _>(&sql, args);

            query.fetch_one(executor).await.map_err(|e| crate::Error::Database(e.into()))
        }
    }

    async fn execute<'e, E>(&self, executor: E) -> Result<(), crate::Error>
    where
        E: Executor<'e, Database = Sqlite> + Send,
    {
        let (sql, args_fn) = self.build_sql();
        tracing::info!("execute: {}", sql);

        let mut args = SqliteArguments::default();
        for f in args_fn {
            f(&mut args);
        }
        let query = sqlx::query_with(&sql, args);

        query.execute(executor).await.map(|_| ()).map_err(|e| crate::Error::Database(e.into()))
    }
}
