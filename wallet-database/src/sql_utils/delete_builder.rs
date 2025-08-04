use crate::sql_utils::SqlArg;

#[derive(Debug)]
pub struct DynamicDeleteBuilder {
    table: String,
    where_clauses: Vec<String>,
    params: Vec<SqlArg>,
}

impl DynamicDeleteBuilder {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clauses: Vec::new(),
            params: Vec::new(),
        }
    }

    pub fn and_where_eq(&mut self, field: &str, arg: SqlArg) {
        self.where_clauses.push(format!("{} = ?", field));
        self.params.push(arg);
    }

    pub fn and_where_like(&mut self, field: &str, keyword: &str) {
        self.where_clauses.push(format!("{} LIKE ?", field));
        self.params.push(SqlArg::Str(format!("%{}%", keyword)));
    }

    pub fn and_where_in<T: ToString>(&mut self, field: &str, values: &[T]) {
        if values.is_empty() {
            return;
        }

        let placeholders = std::iter::repeat("?")
            .take(values.len())
            .collect::<Vec<_>>()
            .join(", ");
        self.where_clauses
            .push(format!("{} IN ({})", field, placeholders));

        for v in values {
            self.params.push(SqlArg::Str(v.to_string()));
        }
    }
}

#[async_trait::async_trait]
impl super::SqlExecutableNoReturn for DynamicDeleteBuilder {}

#[async_trait::async_trait]
impl<T> super::SqlExecutableReturn<T> for DynamicDeleteBuilder where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}

#[async_trait::async_trait]
impl super::SqlBuilder for DynamicDeleteBuilder {
    fn build(&self) -> (String, Vec<SqlArg>) {
        let mut sql = format!("DELETE FROM {}", self.table);
        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }
        (sql, self.params.clone())
    }
}
