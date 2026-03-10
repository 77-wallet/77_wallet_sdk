use sqlx::Sqlite;

pub type DbPool = std::sync::Arc<sqlx::Pool<Sqlite>>;

#[derive(Clone, Debug)]
struct SplitDbPool {
    read_pool: DbPool,
    write_pool: DbPool,
}

#[derive(Clone, Debug)]
pub struct CoreDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct TaskDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct ApiFundsDbPool(std::sync::Arc<SplitDbPool>);

#[derive(Clone, Debug)]
pub struct ApiWalletDbPool(std::sync::Arc<SplitDbPool>);

macro_rules! impl_split_pool_wrapper {
    ($name:ident) => {
        impl $name {
            // Backward compatible constructor: read/write share the same pool.
            pub fn new(pool: DbPool) -> Self {
                Self::new_split(pool.clone(), pool)
            }

            pub fn new_split(read_pool: DbPool, write_pool: DbPool) -> Self {
                Self(std::sync::Arc::new(SplitDbPool { read_pool, write_pool }))
            }

            pub fn as_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.read_pool.as_ref()
            }

            pub fn read_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.read_pool.as_ref()
            }

            pub fn write_ref(&self) -> &sqlx::Pool<Sqlite> {
                self.0.write_pool.as_ref()
            }

            pub fn read_pool(&self) -> DbPool {
                self.0.read_pool.clone()
            }

            pub fn write_pool(&self) -> DbPool {
                self.0.write_pool.clone()
            }

            // Compatibility helper used by legacy transaction code paths.
            pub fn into_inner(&self) -> DbPool {
                self.0.write_pool.clone()
            }
        }
    };
}

impl_split_pool_wrapper!(CoreDbPool);
impl_split_pool_wrapper!(TaskDbPool);
impl_split_pool_wrapper!(ApiFundsDbPool);
impl_split_pool_wrapper!(ApiWalletDbPool);
