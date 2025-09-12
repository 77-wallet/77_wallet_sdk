use wallet_database::{
    DbPool,
    dao::multisig_account::MultisigAccountDaoV1,
    entities::{
        multisig_account::{MultiAccountOwner, MultisigAccountStatus, NewMultisigAccountEntity},
        multisig_member::MemberVo,
    },
    factory::RepositoryFactory,
    repositories::{ResourcesRepo, account::AccountRepoTrait, wallet::WalletRepoTrait},
};

use crate::{
    context::Context,
    messaging::{
        notify::{FrontendNotifyEvent, event::NotifyEvent, multisig::OrderMultiSignAcceptFrontend},
        system_notification::{Notification, NotificationType},
    },
    service::system_notification::SystemNotificationService,
};

// 后台将多签账号的数据同步给其他参数方消息(第一步)
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignAccept {
    /// uuid
    pub(crate) id: String,
    // order_id
    pub(crate) name: String,
    /// 发起方地址
    pub(crate) initiator_addr: String,
    /// 多签钱包地址
    pub(crate) address: String,
    /// 链编码
    pub(crate) chain_code: String,
    /// 签名阀值
    pub(crate) threshold: i32,
    pub(crate) address_type: String,
    #[serde(rename = "memeber")]
    pub(crate) member: Vec<MemberVo>,
}

impl OrderMultiSignAccept {
    pub fn to_json_str(&self) -> Result<String, crate::error::ServiceError> {
        Ok(wallet_utils::serde_func::serde_to_string(self)?)
    }

    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG".to_string()
    }
}

// impl OrderMultiSignAccept {
//     pub fn member_lists(&self) -> Vec<String> {
//         self.member
//             .iter()
//             .map(|i| i.address.to_string())
//             .collect::<Vec<String>>()
//     }
// }

impl From<&NewMultisigAccountEntity> for OrderMultiSignAccept {
    fn from(value: &NewMultisigAccountEntity) -> Self {
        let list = value
            .member_list
            .iter()
            .map(|i| MemberVo {
                name: i.name.to_owned(),
                address: i.address.to_owned(),
                confirmed: i.confirmed,
                pubkey: i.pubkey.clone(),
                uid: i.uid.clone(),
            })
            .collect::<Vec<MemberVo>>();
        Self {
            id: value.id.to_owned(),
            name: value.name.to_owned(),
            initiator_addr: value.initiator_addr.to_owned(),
            address: value.address.to_owned(),
            chain_code: value.chain_code.to_owned(),
            threshold: value.threshold,
            address_type: value.address_type.to_string(),
            member: list,
        }
    }
}

impl OrderMultiSignAccept {
    async fn check_if_cancelled(id: &str) -> Result<bool, crate::ServiceError> {
        tracing::info!("Checking if multisig account {} is cancelled...", id);
        let backend_api = crate::Context::get_global_backend_api()?;
        let is_cancel = backend_api.check_multisig_account_is_cancel(id).await?;
        tracing::info!("Multisig account {} cancellation status: {}", id, is_cancel.status);
        Ok(is_cancel.status)
    }

    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignAccept"
        );

        let pool = Context::get_global_sqlite_pool()?;
        let mut repo = RepositoryFactory::repo(pool.clone());

        let account = AccountRepoTrait::detail(&mut repo, &self.address).await?;

        let uid_list = repo.uid_list().await?.into_iter().map(|uid| uid.0).collect();

        let mut params = NewMultisigAccountEntity::new(
            Some(self.id.clone()),
            self.name.clone(),
            self.initiator_addr.to_string(),
            self.address.to_string(),
            self.chain_code.to_string(),
            self.threshold,
            self.address_type.to_string(),
            self.member.to_owned(),
            &uid_list,
        );

        // 如果查到该账号，说明是自己，修改Owner为自己
        params.owner = match account {
            Some(_) => MultiAccountOwner::Owner,
            None => MultiAccountOwner::Participant,
        };

        Self::update_member_info(&mut repo, &mut params).await?;
        tracing::info!(
            event_name = %event_name,
            "Update member info for account {}",self.id);

        Self::crate_multisig_account(&pool, params).await?;

        // 查询后端接口，判断是否账户已被取消
        if Self::check_if_cancelled(&self.id).await? {
            tracing::warn!(
                event_name = %event_name,
                "Multisig Account {} has been canceled",self.id);
            MultisigAccountDaoV1::delete_in_status(&self.id, &*pool)
                .await
                .map_err(|e| crate::ServiceError::Database(e.into()))?;
        }

        tracing::info!(
            event_name = %event_name,
            "Sync multisig for account {}",self.id);
        // Self::send_system_notification(msg_id, name, address, id).await?;

        Self::send_to_frontend(&self).await?;
        Ok(())
    }

    async fn update_member_info(
        repo: &mut ResourcesRepo,
        params: &mut NewMultisigAccountEntity,
    ) -> Result<(), crate::ServiceError> {
        let mut status = MultisigAccountStatus::Confirmed;
        for m in params.member_list.iter_mut() {
            if m.confirmed != 1 {
                status = MultisigAccountStatus::Pending;
            }

            // 查询每个成员的账号，如果查到，说明是自己，修改为是自己
            let account = AccountRepoTrait::detail(repo, &m.address).await?;
            if account.is_some() {
                m.is_self = 1;
            }

            if params.owner == MultiAccountOwner::Owner
                && m.is_self == 1
                && m.address != params.initiator_addr
            {
                params.owner = MultiAccountOwner::Both;
            }
        }
        params.status = status;
        Ok(())
    }

    async fn send_to_frontend(accept: &OrderMultiSignAccept) -> Result<(), crate::ServiceError> {
        let data = NotifyEvent::OrderMultiSignAccept(OrderMultiSignAcceptFrontend {
            name: accept.name.to_string(),
            initiator_addr: accept.initiator_addr.to_string(),
            address: accept.address.to_string(),
            chain_code: accept.chain_code.to_string(),
            threshold: accept.threshold,
            member: accept.member.to_vec(),
        });
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    async fn crate_multisig_account(
        pool: &DbPool,
        params: NewMultisigAccountEntity,
    ) -> Result<(), crate::ServiceError> {
        let account = MultisigAccountDaoV1::find_by_id(&params.id, pool.as_ref()).await?;
        if account.is_none() {
            // 创建多签账户以及多签成员
            MultisigAccountDaoV1::create_account_with_member(&params, pool.clone()).await?;
            tracing::info!("Multisig account {} created.", params.id);
        }
        Ok(())
    }

    async fn _send_system_notification(
        msg_id: &str,
        account_name: &str,
        account_address: &str,
        multisig_account_id: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = Context::get_global_sqlite_pool()?;
        let repo = RepositoryFactory::repo(pool);
        let notification = Notification::new_multisig_notification(
            account_name,
            account_address,
            multisig_account_id,
            NotificationType::DeployInvite,
        );
        let system_notification_service = SystemNotificationService::new(repo);

        system_notification_service.add_system_notification(msg_id, notification, 0).await?;
        Ok(())
    }
}
