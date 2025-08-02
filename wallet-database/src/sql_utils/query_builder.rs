use sqlx::{query_as, Sqlite};
use std::fmt::Debug;

#[derive(Debug)]
pub struct DynamicQueryBuilder {
    base_sql: String,
    where_clauses: Vec<String>,
    order_by: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    params: Vec<SqlArg>,
}

#[derive(Debug, Clone)]
pub enum SqlArg {
    Str(String),
    Int(i64),
    Bool(bool),
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

    pub fn and_where(&mut self, clause: &str, arg: SqlArg) {
        self.where_clauses.push(clause.to_string());
        self.params.push(arg);
    }

    pub fn and_where_eq(&mut self, field: &str, arg: SqlArg) {
        let clause = format!("{} = ?", field);
        self.where_clauses.push(clause);
        self.params.push(arg);
    }

    pub fn and_where_in<T: ToString>(&mut self, field: &str, values: &[T]) {
        if values.is_empty() {
            return;
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
    }

    pub fn order_by(&mut self, order: &str) {
        self.order_by = Some(order.to_string());
    }

    pub fn limit(&mut self, limit: u32) {
        self.limit = Some(limit);
    }

    pub fn offset(&mut self, offset: u32) {
        self.offset = Some(offset);
    }

    pub fn build(&self) -> (String, Vec<SqlArg>) {
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

pub async fn execute_query_as<'e, E, T>(
    executor: E,
    builder: &DynamicQueryBuilder,
) -> Result<Vec<T>, crate::Error>
where
    E: sqlx::Executor<'e, Database = Sqlite>,
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin,
{
    let (sql, args) = builder.build();

    tracing::info!("sql: {sql}");

    let mut query = query_as::<_, T>(&sql);

    // tracing::info!("args: {args:#?}");
    for arg in args {
        query = match arg {
            SqlArg::Str(s) => query.bind(s),
            SqlArg::Int(i) => query.bind(i),
            SqlArg::Bool(b) => query.bind(b),
        };
    }

    query
        .fetch_all(executor)
        .await
        .map_err(|e| crate::Error::Database(e.into()))
}
