use std::sync::Arc;

use sqlx::{Arguments as _, Sqlite};

use crate::sql_utils::ArgFn;

pub struct DynamicQueryBuilder<'a> {
    base_sql: String,
    joins: Vec<String>,
    where_clauses: Vec<String>,
    order_by: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    arg_fns: Vec<ArgFn<'a>>,
}

impl<'a> DynamicQueryBuilder<'a> {
    pub fn new(base_sql: &str) -> Self {
        Self {
            base_sql: base_sql.to_string(),
            joins: vec![],
            where_clauses: vec![],
            order_by: None,
            limit: None,
            offset: None,
            arg_fns: vec![],
        }
    }

    pub fn left_join(mut self, clause: &str) -> Self {
        self.joins.push(format!("LEFT JOIN {}", clause));
        self
    }

    pub fn right_join(mut self, clause: &str) -> Self {
        self.joins.push(format!("RIGHT JOIN {}", clause));
        self
    }

    pub fn inner_join(mut self, clause: &str) -> Self {
        self.joins.push(format!("INNER JOIN {}", clause));
        self
    }

    pub fn and_where<T>(mut self, clause: &str, val: T) -> Self
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, Sqlite> + sqlx::Type<Sqlite>,
    {
        self.where_clauses.push(clause.to_string());

        self.arg_fns.push(Arc::new(move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
            args.add(val.clone());
        }));
        self
    }

    pub fn and_where_eq<T>(mut self, field: &str, val: T) -> Self
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, Sqlite> + sqlx::Type<Sqlite>,
    {
        let clause = format!("{} = ?", field);
        self.where_clauses.push(clause);

        self.arg_fns.push(Arc::new(move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
            args.add(val.clone());
        }));
        self
    }

    pub fn and_where_eq_opt<T>(mut self, field: &str, val: Option<T>) -> Self
    where
        T: Clone + Send + Sync + 'a + sqlx::Encode<'a, Sqlite> + sqlx::Type<Sqlite>,
    {
        if let Some(v) = val {
            self = self.and_where_eq(field, v);
        }
        self
    }

    pub fn and_where_like(mut self, field: &str, keyword: &str) -> Self {
        let clause = format!("{} LIKE ?", field);
        let pattern = format!("%{}%", keyword);
        self.where_clauses.push(clause);

        self.arg_fns.push(Arc::new(move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
            args.add(pattern.clone());
        }));
        self
    }

    pub fn and_where_in<T>(mut self, field: &str, values: &[T]) -> Self
    where
        T: ToString + Send + Sync + 'static,
    {
        if values.is_empty() {
            return self;
        }

        let placeholders = (0..values.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
        let clause = format!("{} IN ({})", field, placeholders);
        self.where_clauses.push(clause);

        for v in values {
            let s = v.to_string();
            self.arg_fns.push(Arc::new(move |args: &mut sqlx::sqlite::SqliteArguments<'a>| {
                args.add(s.clone());
            }));
        }

        self
    }

    pub fn order_by(mut self, order: &str) -> Self {
        self.order_by = Some(order.to_string());
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl<'q> super::SqlQueryBuilder<'q> for DynamicQueryBuilder<'q> {
    fn build_sql(&self) -> (String, Vec<ArgFn<'q>>) {
        let mut sql = self.base_sql.to_string();

        if !self.joins.is_empty() {
            sql.push(' ');
            sql.push_str(&self.joins.join(" "));
        }

        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }

        if let Some(order) = &self.order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(order);
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let arg_fns = self.arg_fns.iter().cloned().map(|f| f as ArgFn<'q>).collect();
        (sql, arg_fns)
    }
}

#[async_trait::async_trait]
impl<'a, T> super::SqlExecutableReturn<'a, T> for DynamicQueryBuilder<'a> where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}
