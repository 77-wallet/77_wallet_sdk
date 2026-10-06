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

    /// API wallet message center's business reminders across active API withdrawal wallets.
    pub async fn page_api_business_notifications(
        &self,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiBusinessNotification>> {
        ApiBusinessNotificationService::new(self.ctx)
            .list_for_withdrawal_wallets(false, page, page_size)
            .await
    }

    /// Unresolved business notifications across active API withdrawal wallets with balance refresh.
    pub async fn page_unresolved_api_business_notifications(
        &self,
        page: i64,
        page_size: i64,
    ) -> ReturnType<Pagination<ApiBusinessNotification>> {
        ApiBusinessNotificationService::new(self.ctx)
            .list_unresolved_for_withdrawal_wallets_with_refresh(page, page_size)
            .await
    }

    pub async fn mark_api_business_notification_read(&self, id: &str) -> ReturnType<()> {
        ApiBusinessNotificationService::new(self.ctx).mark_read_for_withdrawal_wallets(id).await
    }

    /// Count unread business reminders, including resolved history.
    pub async fn count_unread_api_business_notifications(&self) -> ReturnType<i64> {
        ApiBusinessNotificationService::new(self.ctx).unread_count_for_withdrawal_wallets().await
    }

    /// Mark all business reminders read without resolving notifications.
    pub async fn mark_all_api_business_notifications_read(&self) -> ReturnType<()> {
        ApiBusinessNotificationService::new(self.ctx).mark_all_read_for_withdrawal_wallets().await
    }
}
