pub mod account;
pub mod address_book;
pub mod announcement;

pub mod api_wallet;

pub mod assets;
pub mod bill;
pub mod chain;
pub mod coin;
pub mod device;
pub mod exchange_rate;
pub mod multisig_account;
pub mod multisig_member;
pub mod multisig_queue;
pub mod node;
pub mod permission;
pub mod stake;
pub mod system_notification;
pub mod task_queue;
pub mod wallet;

pub struct RepoCtx {
    db_pool: crate::DbPool,
    transaction: Option<sqlx::Transaction<'static, sqlx::Sqlite>>,
}

impl RepoCtx {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self { db_pool, transaction: None }
    }

    pub fn pool(&self) -> crate::DbPool {
        self.db_pool.clone()
    }

    pub fn pool_ref(&self) -> &crate::DbPool {
        &self.db_pool
    }
}

pub type ResourcesRepo = RepoCtx;

pub struct UnitOfWork {
    ctx: RepoCtx,
}

impl UnitOfWork {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self { ctx: RepoCtx::new(db_pool) }
    }

    pub fn from_ctx(ctx: RepoCtx) -> Self {
        Self { ctx }
    }

    pub fn into_ctx(self) -> RepoCtx {
        self.ctx
    }

    pub fn ctx_mut(&mut self) -> &mut RepoCtx {
        &mut self.ctx
    }

    pub fn pool_ref(&self) -> &crate::DbPool {
        self.ctx.pool_ref()
    }

    pub async fn begin(&mut self) -> Result<(), crate::Error> {
        let tx = self.ctx.db_pool.begin().await.map_err(|e| crate::Error::Database(e.into()))?;
        self.ctx.transaction = Some(tx);
        Ok(())
    }

    pub async fn commit(&mut self) -> Result<(), crate::Error> {
        if let Some(transaction) = self.ctx.transaction.take() {
            transaction.commit().await.map_err(|e| crate::Error::Database(e.into()))?;
        }
        Ok(())
    }

    pub async fn rollback(&mut self) -> Result<(), crate::Error> {
        if let Some(transaction) = self.ctx.transaction.take() {
            transaction.rollback().await.map_err(|e| crate::Error::Database(e.into()))?;
        }
        Ok(())
    }

    pub fn executor(&mut self) -> Result<ExecutorWrapper<'_>, crate::Error> {
        if let Some(tx) = self.ctx.transaction.as_mut() {
            Ok(ExecutorWrapper::Transaction(tx))
        } else {
            Ok(ExecutorWrapper::Pool(&self.ctx.db_pool))
        }
    }
}

impl RepoCtx {
    pub async fn begin(&mut self) -> Result<(), crate::Error> {
        let tx = self.db_pool.begin().await.map_err(|e| crate::Error::Database(e.into()))?;
        self.transaction = Some(tx);
        Ok(())
    }

    pub async fn commit(&mut self) -> Result<(), crate::Error> {
        if let Some(transaction) = self.transaction.take() {
            transaction.commit().await.map_err(|e| crate::Error::Database(e.into()))?;
        }
        Ok(())
    }

    pub fn executor(&mut self) -> Result<ExecutorWrapper<'_>, crate::Error> {
        if let Some(tx) = self.transaction.as_mut() {
            Ok(ExecutorWrapper::Transaction(tx))
        } else {
            Ok(ExecutorWrapper::Pool(&self.db_pool))
        }
    }
}

#[async_trait::async_trait]
impl TransactionTrait for RepoCtx {
    async fn begin_transaction(&mut self) -> Result<(), crate::Error>
    where
        Self: Sized,
    {
        self.begin().await
    }

    async fn commit_transaction(&mut self) -> Result<(), crate::Error>
    where
        Self: Sized,
    {
        self.commit().await
    }

    fn get_conn_or_tx(&mut self) -> Result<ExecutorWrapper<'_>, crate::Error> {
        self.executor()
    }

    fn get_db_pool(&self) -> &crate::DbPool {
        &self.db_pool
    }
}

pub enum ExecutorWrapper<'a> {
    Transaction(&'a mut sqlx::Transaction<'static, sqlx::Sqlite>),
    Pool(&'a sqlx::Pool<sqlx::Sqlite>),
}

impl ExecutorWrapper<'_> {
    pub async fn execute<F, Fut, T>(self, query: F) -> Result<T, crate::Error>
    where
        F: for<'c> FnOnce(&'c mut sqlx::SqliteConnection) -> Fut,
        Fut: std::future::Future<Output = Result<T, crate::Error>>,
    {
        match self {
            ExecutorWrapper::Transaction(executor) => query(executor.as_mut()).await,
            ExecutorWrapper::Pool(executor) => {
                let mut conn =
                    executor.acquire().await.map_err(|e| crate::Error::Database(e.into()))?;
                query(&mut conn).await
            }
        }
    }
}

#[async_trait::async_trait]
pub trait TransactionTrait: std::marker::Send {
    async fn begin_transaction(&mut self) -> Result<(), crate::Error>
    where
        Self: Sized;
    // {
    //     let conn = self.get_db_pool();
    //     let tx = conn
    //         .begin()
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))?;
    //     self.insert_transaction(tx);
    //     Ok(self)
    // }

    async fn commit_transaction(&mut self) -> Result<(), crate::Error>
    where
        Self: Sized;
    // {
    //     let conn = self.get_transaction()?;
    //     conn.commit()
    //         .await
    //         .map_err(|e| crate::Error::Database(e.into()))?;

    //     Ok(())
    // }

    fn get_conn_or_tx(&mut self) -> Result<ExecutorWrapper<'_>, crate::Error>;
    fn get_db_pool(&self) -> &crate::DbPool;
}
