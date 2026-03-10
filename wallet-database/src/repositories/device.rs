use crate::{CoreDbPool, dao::device::DeviceDao, entities::device::DeviceEntity};
use sqlx::{Sqlite, Transaction};

pub struct DeviceRepo;

impl DeviceRepo {
    pub async fn get_device_info(
        pool: CoreDbPool,
        sn: &str,
    ) -> Result<Option<DeviceEntity>, crate::Error> {
        Ok(DeviceDao::get_device_info(pool.as_ref(), sn).await?)
    }

    pub async fn update_uid(
        pool: CoreDbPool,
        sn: &str,
        uid: Option<&str>,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_uid(pool.as_ref(), sn, uid).await
    }

    pub async fn update_uid_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        sn: &str,
        uid: Option<&str>,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_uid(tx.as_mut(), sn, uid).await
    }

    pub async fn upsert(
        pool: CoreDbPool,
        req: crate::entities::device::CreateDeviceEntity,
    ) -> Result<DeviceEntity, crate::Error> {
        DeviceDao::upsert(pool.as_ref(), req).await
    }

    pub async fn update_password(
        pool: CoreDbPool,
        sn: &str,
        password: Option<&str>,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_password(pool.as_ref(), sn, password).await
    }

    pub async fn update_password_with_executor(
        tx: &mut Transaction<'_, Sqlite>,
        sn: &str,
        password: Option<&str>,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_password(tx.as_mut(), sn, password).await
    }

    pub async fn update_password_proof(
        pool: CoreDbPool,
        sn: &str,
        password_proof: Option<&str>,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_password_proof(pool.as_ref(), sn, password_proof).await
    }

    pub async fn update_app_id(
        pool: CoreDbPool,
        sn: &str,
        app_id: &str,
    ) -> Result<(), crate::Error> {
        DeviceDao::update_app_id(pool.as_ref(), sn, app_id).await
    }

    pub async fn device_init(pool: CoreDbPool, sn: &str) -> Result<(), crate::Error> {
        DeviceDao::init(pool.as_ref(), sn).await
    }

    pub async fn language_init(pool: CoreDbPool, sn: &str) -> Result<(), crate::Error> {
        DeviceDao::language_init(pool.as_ref(), sn).await
    }
}
