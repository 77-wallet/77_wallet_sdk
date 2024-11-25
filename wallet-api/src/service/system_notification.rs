use wallet_database::{
    entities::system_notification::{CreateSystemNotificationEntity, SystemNotificationEntity},
    repositories::system_notification::SystemNotificationRepoTrait,
};

pub struct SystemNotificationService<T: SystemNotificationRepoTrait> {
    pub repo: T,
}

impl<T: SystemNotificationRepoTrait> SystemNotificationService<T> {
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn add_system_notification(
        self,
        id: &str,
        notification: crate::system_notification::Notification,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let r#type = notification.type_name();
        let content = notification.serialize()?;
        tx.upsert(id, &r#type, content, status)
            .await
            .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))?;

        Ok(())
    }

    pub async fn add_system_notification_with_key_value(
        self,
        id: &str,
        notification: crate::system_notification::Notification,
        status: i8,
        key: Option<String>,
        value: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let r#type = notification.type_name();
        let content = notification.serialize()?;
        tx.upsert_with_key_value(id, &r#type, content, status, key, value)
            .await
            .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))?;
        Ok(())
    }

    pub async fn add_multi_system_notification_with_key_value(
        self,
        reqs: &[CreateSystemNotificationEntity],
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        tx.upsert_multi_with_key_value(reqs)
            .await
            .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))?;
        Ok(())
    }

    pub async fn update_system_notification_status(
        self,
        id: Option<String>,
        status: i8,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        tx.update_status(id, status)
            .await
            .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))?;

        Ok(())
    }

    pub async fn get_system_notification_list(
        self,
        page: i64,
        page_size: i64,
    ) -> Result<
        wallet_database::pagination::Pagination<SystemNotificationEntity>,
        crate::ServiceError,
    > {
        let mut tx = self.repo;
        tx.list(page, page_size)
            .await
            .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))
    }
}

// use crate::global_context::GlobalContext;

// use super::Service;

// impl Service {
//     pub async fn add_system_notification(
//         &self,
//         r#type: i8,
//         content: String,
//         status: i8,
//     ) -> Result<(), crate::ServiceError> {
//         self.get_global_sqlite_context()?
//             .add_system_notification(r#type, content, status)
//             .await
//             .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))?;

//         Ok(())
//     }

//     pub async fn update_system_notification_status(
//         &self,
//         id: Option<i32>,
//         status: i8,
//     ) -> Result<(), crate::ServiceError> {
//         self.get_global_sqlite_context()?
//             .update_system_notification_status(id, status)
//             .await
//             .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))
//     }

//     pub async fn get_system_notification_list(
//         &self,
//         page: i64,
//         page_size: i64,
//     ) -> Result<
//         wallet_database::pagination::Pagination<
//             wallet_database::sqlite::logic::system_notification::SystemNotificationEntity,
//         >,
//         crate::ServiceError,
//     > {
//         self.get_global_sqlite_context()?
//             .system_notification_list(page, page_size)
//             .await
//             .map_err(|e| crate::ServiceError::System(crate::SystemError::Database(e)))
//     }
// }
