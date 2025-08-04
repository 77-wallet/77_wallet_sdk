use crate::sql_utils::SqlArg;

#[derive(Debug)]
pub struct DynamicUpdateBuilder {
    table: String,
    set_clauses: Vec<String>,
    where_clauses: Vec<String>,
    params: Vec<SqlArg>,
}

impl DynamicUpdateBuilder {
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            set_clauses: vec![],
            where_clauses: vec![],
            params: vec![],
        }
    }

    pub fn set(&mut self, field: &str, arg: SqlArg) {
        self.set_clauses.push(format!("{} = ?", field));
        self.params.push(arg);
    }

    pub fn set_raw(&mut self, expr: &str) {
        // like: updated_at = strftime(...)
        self.set_clauses.push(expr.to_string());
    }

    pub fn and_where_eq(&mut self, field: &str, arg: SqlArg) {
        self.where_clauses.push(format!("{} = ?", field));
        self.params.push(arg);
    }
}

#[async_trait::async_trait]
impl<T> super::SqlExecutableReturn<T> for DynamicUpdateBuilder where
    T: for<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> + Send + Unpin + 'static
{
}

#[async_trait::async_trait]
impl super::SqlBuilder for DynamicUpdateBuilder {
    fn build(&self) -> (String, Vec<SqlArg>) {
        let mut sql = format!("UPDATE {} SET ", self.table);
        sql.push_str(&self.set_clauses.join(", "));

        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.where_clauses.join(" AND "));
        }

        sql.push_str(" RETURNING *");

        (sql, self.params.clone())
    }
}
