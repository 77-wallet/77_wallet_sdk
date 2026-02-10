use sqlx::Sqlite;

pub type DbPool = std::sync::Arc<sqlx::Pool<Sqlite>>;

#[derive(Clone, Debug)]
pub struct CoreDbPool(DbPool);

#[derive(Clone, Debug)]
pub struct TaskDbPool(DbPool);

#[derive(Clone, Debug)]
pub struct ApiFundsDbPool(DbPool);
#[derive(Clone, Debug)]
pub struct ApiWalletDbPool(DbPool);

impl CoreDbPool {
    pub fn new(pool: DbPool) -> Self {
        Self(pool)
    }

    pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
        self.0.as_ref()
    }

    pub fn into_inner(&self) -> DbPool {
        self.0.clone()
    }
}

impl TaskDbPool {
    pub fn new(pool: DbPool) -> Self {
        Self(pool)
    }

    pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
        self.0.as_ref()
    }

    pub fn into_inner(&self) -> DbPool {
        self.0.clone()
    }
}

impl ApiFundsDbPool {
    pub fn new(pool: DbPool) -> Self {
        Self(pool)
    }

    pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
        self.0.as_ref()
    }

    pub fn into_inner(&self) -> DbPool {
        self.0.clone()
    }
}

impl ApiWalletDbPool {
    pub fn new(pool: DbPool) -> Self {
        Self(pool)
    }

    pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
        self.0.as_ref()
    }

    pub fn into_inner(&self) -> DbPool {
        self.0.clone()
    }
}
