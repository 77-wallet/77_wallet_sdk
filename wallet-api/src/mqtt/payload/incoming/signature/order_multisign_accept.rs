use wallet_database::{
    dao::multisig_account::MultisigAccountDaoV1,
    entities::{
        multisig_account::{MultiAccountOwner, MultisigAccountStatus, NewMultisigAccountEntity},
        multisig_member::MemberVo,
    },
    factory::RepositoryFactory,
    repositories::{account::AccountRepoTrait, wallet::WalletRepoTrait, ResourcesRepo},
};

use crate::{
    manager::Context,
    notify::event::multisig::OrderMultiSignAcceptFrontend,
    service::system_notification::SystemNotificationService,
    system_notification::{Notification, NotificationType},
};

// 后台将多签账号的数据同步给其他参数方消息(第一步)
use super::OrderMultiSignAccept;

impl OrderMultiSignAccept {
    pub fn member_lists(&self) -> Vec<String> {
        self.memeber
            .iter()
            .map(|i| i.address.to_string())
            .collect::<Vec<String>>()
    }
}

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
            memeber: list,
        }
    }
}

impl OrderMultiSignAccept {
    async fn check_if_cancelled(id: &str) -> Result<bool, crate::ServiceError> {
        tracing::info!("Checking if multisig account {} is cancelled...", id);
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let is_cancel = backend_api
            .check_multisig_account_is_cancel(cryptor, id)
            .await?;
        tracing::info!(
            "Multisig account {} cancellation status: {}",
            id,
            is_cancel.status
        );
        Ok(is_cancel.status)
    }

    pub(crate) async fn exec(self, msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignAccept"
        );

        let OrderMultiSignAccept {
            ref id,
            ref name,
            ref initiator_addr,
            ref address,
            ref chain_code,
            threshold,
            ref address_type,
            ref memeber,
        } = self;

        let pool = Context::get_global_sqlite_pool()?;
        let mut repo = RepositoryFactory::repo(pool.clone());
        // 查询后端接口，判断是否账户已被取消
        if Self::check_if_cancelled(id).await? {
            tracing::warn!(
                event_name = %event_name,
                "Multisig Account {id} has been canceled");
            return Ok(());
        }

        let account = AccountRepoTrait::detail(&mut repo, address).await?;

        let uid_list = repo
            .uid_list()
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect();

        let mut params = NewMultisigAccountEntity::new(
            Some(id.to_string()),
            name.to_string(),
            initiator_addr.to_string(),
            address.to_string(),
            chain_code.to_string(),
            threshold,
            address_type.to_string(),
            memeber.to_owned(),
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
            "Update member info for account {id}");

        Self::sync_multisig_account(params).await?;

        tracing::info!(
            event_name = %event_name,
            "Sync multisig for account {id}");
        Self::send_system_notification(msg_id, name, address, &id).await?;

        Self::send_to_frontend(
            name,
            initiator_addr,
            address,
            chain_code,
            threshold,
            memeber,
        )
        .await?;
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

    async fn send_system_notification(
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

        system_notification_service
            .add_system_notification(msg_id, notification, 0)
            .await?;
        Ok(())
    }

    async fn send_to_frontend(
        name: &str,
        initiator_addr: &str,
        address: &str,
        chain_code: &str,
        threshold: i32,
        memeber: &[MemberVo],
    ) -> Result<(), crate::ServiceError> {
        let data = crate::notify::NotifyEvent::OrderMultiSignAccept(OrderMultiSignAcceptFrontend {
            name: name.to_string(),
            initiator_addr: initiator_addr.to_string(),
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            threshold,
            memeber: memeber.to_vec(),
        });
        crate::notify::FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    async fn sync_multisig_account(
        params: NewMultisigAccountEntity,
    ) -> Result<(), crate::ServiceError> {
        let pool = Context::get_global_sqlite_pool()?;
        let account = MultisigAccountDaoV1::find_by_id(&params.id, pool.as_ref()).await?;
        if account.is_none() {
            // 创建多签账户以及多签成员
            MultisigAccountDaoV1::create_account_with_member(&params, pool.clone()).await?;
            tracing::info!("Multisig account {} created.", params.id);
        }
        Ok(())
    }
}
