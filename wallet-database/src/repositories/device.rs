use crate::{DbPool, entities::device::DeviceEntity};

pub struct DeviceRepo;

impl DeviceRepo {
    pub async fn get_device_info(
        pool: DbPool,
        sn: &str,
    ) -> Result<Option<DeviceEntity>, crate::Error> {
        Ok(DeviceEntity::get_device_info(pool.as_ref(), sn).await?)
    }

    pub async fn update_uid<'a, E>(
        executor: E,
        sn: &str,
        uid: Option<&str>,
    ) -> Result<(), crate::Error>
    where
        E: sqlx::Executor<'a, Database = sqlx::Sqlite>,
    {
        DeviceEntity::update_uid(executor, sn, uid).await
    }
}

#[async_trait::async_trait]
pub trait DeviceRepoTrait: super::TransactionTrait {
    async fn upsert(
        &mut self,
        req: crate::entities::device::CreateDeviceEntity,
    ) -> Result<DeviceEntity, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, DeviceEntity::upsert, req)
    }

    async fn update_password(
        &mut self,
        sn: &str,
        password: Option<&str>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, DeviceEntity::update_password, sn, password)
    }

    async fn update_password_proof(
        &mut self,
        sn: &str,
        password_proof: Option<&str>,
    ) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            DeviceEntity::update_password_proof,
            sn,
            password_proof
        )
    }

    // async fn update_uid(&mut self, uid: Option<&str>) -> Result<(), crate::Error> {
    //     let executor = self.get_conn_or_tx()?;
    //     crate::execute_with_executor!(executor, DeviceEntity::update_uid, uid)
    // }

    async fn update_app_id(&mut self, sn: &str, app_id: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, DeviceEntity::update_app_id, sn, app_id)
    }

    async fn device_init(&mut self, sn: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, DeviceEntity::init, sn)
    }

    async fn language_init(&mut self, sn: &str) -> Result<(), crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, DeviceEntity::language_init, sn)
    }
}
