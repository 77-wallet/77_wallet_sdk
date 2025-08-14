use std::sync::Arc;

use sqlx::Arguments as _;

use crate::sql_utils::ArgFn;

// #[derive(Debug)]
pub struct DynamicDeleteBuilder<'a> {
    table: String,
    where_clauses: Vec<String>,
    arg_fns: Vec<ArgFn<'a>>,
}

impl<'a> DynamicDeleteBuilder<'a> {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clauses: Vec::new(),
            arg_fns: vec![],
        }
    }

    pub fn and_where_eq<T>(mut self, field: &str, val: T) -> Self
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
    {
        self.where_clauses.push(format!("{} = ?", field));
        self.arg_fns.push(Arc::new(
            move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                args.add(val.clone());
            },
        ));
        self
    }

    pub fn and_where_like(mut self, field: &str, keyword: &str) -> Self {
        self.where_clauses.push(format!("{} LIKE ?", field));
        let pattern = format!("%{}%", keyword);
        self.arg_fns.push(Arc::new(
            move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                args.add(pattern.clone());
            },
        ));
        self
    }

    pub fn and_where_in<T>(mut self, field: &str, values: &[T]) -> Self
    where
        T: ToString + Send + 'a,
    {
        if values.is_empty() {
            return self;
        }
        let placeholders = std::iter::repeat("?")
            .take(values.len())
            .collect::<Vec<_>>()
            .join(", ");
        self.where_clauses
            .push(format!("{} IN ({})", field, placeholders));
        for v in values {
            let s = v.to_string();
            self.arg_fns.push(Arc::new(
                move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                    args.add(s.clone());
                },
            ));
        }
        self
    }
}

#[async_trait::async_trait]
impl<'a> super::SqlExecutableNoReturn<'a> for DynamicDeleteBuilder<'a> {}

#[async_trait::async_trait]
impl<'a, T> super::SqlExecutableReturn<'a, T> for DynamicDeleteBuilder<'a> where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}

impl<'q> super::SqlQueryBuilder<'q> for DynamicDeleteBuilder<'q> {
    fn build_sql(&self) -> (String, Vec<ArgFn<'q>>) {
        let mut sql = format!("DELETE FROM {}", self.table);
        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }
        let arg_fns = self
            .arg_fns
            .iter()
            .cloned()
            .map(|f| f as ArgFn<'q>)
            .collect();
        (sql, arg_fns)
    }
}
