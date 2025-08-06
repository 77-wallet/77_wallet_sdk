use std::fmt::Debug;

use crate::sql_utils::SqlArg;

#[derive(Debug)]
pub struct DynamicQueryBuilder {
    base_sql: String,
    where_clauses: Vec<String>,
    order_by: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    params: Vec<SqlArg>,
}

impl DynamicQueryBuilder {
    pub fn new(base_sql: &str) -> Self {
        Self {
            base_sql: base_sql.to_string(),
            where_clauses: Vec::new(),
            order_by: None,
            limit: None,
            offset: None,
            params: Vec::new(),
        }
    }

    pub fn and_where(mut self, clause: &str, arg: SqlArg) -> Self {
        self.where_clauses.push(clause.to_string());
        self.params.push(arg);
        self
    }

    pub fn and_where_eq(mut self, field: &str, arg: SqlArg) -> Self {
        let clause = format!("{} = ?", field);
        self.where_clauses.push(clause);
        self.params.push(arg);
        self
    }

    pub fn and_where_like(mut self, field: &str, keyword: &str) -> Self {
        let clause = format!("{} LIKE ?", field);
        let pattern = format!("%{}%", keyword);
        self.where_clauses.push(clause);
        self.params.push(SqlArg::Str(pattern));
        self
    }

    pub fn and_where_in<T: ToString>(mut self, field: &str, values: &[T]) -> Self {
        if values.is_empty() {
            return self;
        }

        let placeholders = (0..values.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let clause = format!("{} IN ({})", field, placeholders);
        self.where_clauses.push(clause);

        for v in values {
            self.params.push(SqlArg::Str(v.to_string())); // 可扩展为泛型类型
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

#[async_trait::async_trait]
impl<T> super::SqlExecutableReturn<T> for DynamicQueryBuilder where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}

#[async_trait::async_trait]
impl super::SqlBuilder for DynamicQueryBuilder {
    fn build(&self) -> (String, Vec<SqlArg>) {
        let mut sql = self.base_sql.clone();

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

        (sql, self.params.clone())
    }
}
