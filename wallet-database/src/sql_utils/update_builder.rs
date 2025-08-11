use std::sync::Arc;

use crate::sql_utils::ArgFn;
use sqlx::Arguments as _;

pub struct DynamicUpdateBuilder<'a> {
    table: String,
    set_clauses: Vec<String>,
    where_clauses: Vec<String>,
    arg_fns: Vec<ArgFn<'a>>,
}

impl<'a> DynamicUpdateBuilder<'a> {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            set_clauses: vec![],
            where_clauses: vec![],
            arg_fns: vec![],
        }
    }

    pub fn set<T>(&mut self, field: &str, val: T)
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
    {
        self.set_clauses.push(format!("{} = ?", field));

        self.arg_fns.push(Arc::new(
            move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                args.add(val.clone());
            },
        ));
    }

    pub fn set_raw(&mut self, expr: &str) {
        self.set_clauses.push(expr.to_string());
    }

    pub fn and_where_eq<T>(&mut self, field: &str, val: T)
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
    {
        self.where_clauses.push(format!("{} = ?", field));

        self.arg_fns.push(Arc::new(
            move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                args.add(val.clone());
            },
        ));
    }
}

#[async_trait::async_trait]
impl<'a, T> super::SqlExecutableReturn<'a, T> for DynamicUpdateBuilder<'a> where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}

impl<'q> super::SqlQueryBuilder<'q> for DynamicUpdateBuilder<'q> {
    fn build_sql(&self) -> (String, Vec<ArgFn<'q>>) {
        let mut sql = format!("UPDATE {} SET ", self.table);
        sql.push_str(&self.set_clauses.join(", "));

        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }

        sql.push_str(" RETURNING *");
        let arg_fns = self
            .arg_fns
            .iter()
            .cloned()
            .map(|f| f as ArgFn<'q>)
            .collect();
        (sql, arg_fns)
    }
}
