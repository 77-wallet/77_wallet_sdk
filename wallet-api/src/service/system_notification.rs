use crate::{
    messaging::system_notification::{Notification, TransactionNotification},
    response_vo::system_notification::SystemNotification,
};
use wallet_database::{
    dao::bill::BillDao, entities::system_notification::CreateSystemNotificationEntity,
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
        notification: Notification,
        status: i8,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        let r#type = notification.type_name();
        let content = notification.serialize()?;
        tx.upsert(id, &r#type, content, status)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;

        Ok(())
    }

    pub async fn add_system_notification_with_key_value(
        self,
        id: &str,
        notification: Notification,
        status: i8,
        key: Option<String>,
        value: Option<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        let r#type = notification.type_name();
        let content = notification.serialize()?;
        tx.upsert_with_key_value(id, &r#type, content, status, key, value)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;
        Ok(())
    }

    pub async fn add_multi_system_notification_with_key_value(
        self,
        reqs: &[CreateSystemNotificationEntity],
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        tx.upsert_multi_with_key_value(reqs)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;
        Ok(())
    }

    pub async fn update_system_notification_status(
        self,
        id: Option<String>,
        status: i8,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut tx = self.repo;
        tx.update_status(id, status)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;

        Ok(())
    }

    pub async fn get_system_notification_list(
        self,
        page: i64,
        page_size: i64,
    ) -> Result<
        wallet_database::pagination::Pagination<SystemNotification>,
        crate::error::service::ServiceError,
    > {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut tx = self.repo;
        let list = tx
            .list(page, page_size)
            .await
            .map_err(crate::error::service::ServiceError::Database)?;

        let mut res = Vec::new();
        for notify in list.data {
            // 针对目前只有一种交易通知
            let no: TransactionNotification =
                match wallet_utils::serde_func::serde_from_str(&notify.content) {
                    Ok(v) => v,
                    Err(_) => {
                        tracing::warn!("delete notification id = {}", notify.id);
                        tx.delete_system_notification(&notify.id).await?;
                        continue;
                    }
                };

            let val = if no.chain_code.is_empty() | no.to_addr.is_empty() | no.from_addr.is_empty()
            {
                tx.delete_system_notification(&notify.id).await?;
                continue;
            } else {
                let hash = no.transaction_hash;
                match BillDao::get_one_by_hash(&hash, &*pool).await? {
                    Some(_) => (notify, true).into(),
                    None => (notify, false).into(),
                }
            };

            res.push(val);
        }

        let list = wallet_database::pagination::Pagination {
            page,
            page_size,
            total_count: list.total_count,
            data: res,
        };

        Ok(list)
    }
}

// 原来有多个消息类型的代码
// let no: Notification = match wallet_utils::serde_func::serde_from_str(&notif.content) {
//     Ok(v) => v,
//     Err(e) => {
//         tracing::warn!("delete notification id = {}", notif.id);
//         tracing::warn!("delete notification id = {}", e);

//         assert!(false);
//         tx.delete_system_notification(&notif.id).await?;
//         continue;
//     }
// };

// let val = match no {
//     Notification::Multisig(notification) => match MultisigAccountDaoV1::find_by_id(
//         &notification.multisig_account_id,
//         &*pool,
//     )
//     .await?
//     {
//         Some(_) => (notif, true).into(),
//         None => (notif, false).into(),
//     },
//     Notification::Confirmation(notification) => match MultisigAccountDaoV1::find_by_id(
//         &notification.multisig_account_id,
//         &*pool,
//     )
//     .await?
//     {
//         Some(_) => (notif, true).into(),
//         None => (notif, false).into(),
//     },
//     Notification::Transaction(transaction_notification) => {
//         if transaction_notification.chain_code.is_empty()
//             | transaction_notification.to_addr.is_empty()
//             | transaction_notification.from_addr.is_empty()
//         {
//             tx.delete_system_notification(&notif.id).await?;
//             continue;
//         }

//         let hash = transaction_notification.transaction_hash;
//         match BillDao::get_one_by_hash(&hash, &*pool).await? {
//             Some(_) => (notif, true).into(),
//             None => (notif, false).into(),
//         }
//     }
//     Notification::Resource(notification) => match MultisigAccountDaoV1::find_by_id(
//         &notification.multisig_account_id,
//         &*pool,
//     )
//     .await?
//     {
//         Some(_) => (notif, true).into(),
//         None => (notif, false).into(),
//     },
//     Notification::PermissionChange(_notification) => (notif, true).into(),
// };

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
