use crate::{
    api::ReturnType, manager::WalletManager,
    service::api_wallet::business_notification::ApiBusinessNotificationService,
};
use wallet_database::{
    entities::api_business_notification::ApiBusinessNotification, pagination::Pagination,
};

impl WalletManager {
    /// Unresolved address-balance notifications across active API withdrawal wallets.
    pub async fn get_unresolved_api_address_balance_notifications(
        &self,
    ) -> ReturnType<Vec<ApiBusinessNotification>> {
        ApiBusinessNotificationService::new(self.ctx)
            .unresolved_address_balance_notifications()
            .await
    }

    /// API wallet message center's business reminders.
    pub async fn page_api_business_notifications(
        &self,
        uid: &str,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiBusinessNotification>> {
        ApiBusinessNotificationService::new(self.ctx).list(uid, false, page, page_size).await
    }

    /// UID-scoped unresolved business notifications with balance refresh.
    pub async fn page_unresolved_api_business_notifications(
        &self,
        uid: &str,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiBusinessNotification>> {
        ApiBusinessNotificationService::new(self.ctx)
            .list_unresolved_with_refresh(uid, page, page_size)
            .await
    }

    pub async fn mark_api_business_notification_read(&self, uid: &str, id: &str) -> ReturnType<()> {
        ApiBusinessNotificationService::new(self.ctx).mark_read(uid, id).await
    }

    /// Count unread business reminders, including resolved history.
    pub async fn count_unread_api_business_notifications(&self, uid: &str) -> ReturnType<i64> {
        ApiBusinessNotificationService::new(self.ctx).unread_count(uid).await
    }

    /// Mark all business reminders read without resolving notifications.
    pub async fn mark_all_api_business_notifications_read(&self, uid: &str) -> ReturnType<()> {
        ApiBusinessNotificationService::new(self.ctx).mark_all_read(uid).await
    }
}
