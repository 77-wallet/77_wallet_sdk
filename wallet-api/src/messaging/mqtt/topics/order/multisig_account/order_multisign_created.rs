use crate::{
    domain::multisig::MultisigDomain,
    messaging::notify::{
        FrontendNotifyEvent, event::NotifyEvent, multisig::OrderMultiSignCreatedFrontend,
    },
};
use wallet_database::dao::multisig_account::MultisigAccountDaoV1;

/*
    {
        "clientId": "wenjing",
        "sn": "device458",
        "deviceType": "typeC",
        "bizType": "ORDER_MULTI_SIGN_CREATED",
        "body": {
            "multisigAccountId": "order-1",
            "multisigAccountAddress": "asdasdasdasd",
            "addressType": "p2wsh",
            "salt": "asdasd",
            "authorityAddr": "sadasdasd"
        }
    }
*/

// 服务费和部署完成后,所有参与方接受到的消息。
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderMultiSignCreated {
    /// 多签账户id
    pub multisig_account_id: String,
    /// 多签账户地址
    pub multisig_account_address: String,
    /// 地址类型
    pub address_type: String,
    /// btc solana 盐
    pub salt: String,
    /// solana 管理地址
    pub authority_addr: String,
    /// 部署的hash
    pub deploy_hash: String,
    /// 服务费hash
    pub fee_hash: String,
    pub fee_chain: Option<String>,
}

impl OrderMultiSignCreated {
    pub(crate) fn name(&self) -> String {
        "ORDER_MULTI_SIGN_CREATED".to_string()
    }
}

impl OrderMultiSignCreated {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let event_name = self.name();
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        tracing::info!(
            event_name = %event_name,
            ?self,
            "Starting to process OrderMultiSignCreated"
        );
        let OrderMultiSignCreated {
            multisig_account_id,
            multisig_account_address,
            address_type,
            salt,
            authority_addr,
            deploy_hash,
            fee_hash,
            fee_chain,
        } = &self;

        if MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref())
            .await
            .map_err(crate::ServiceError::Database)?
            .is_none()
        {
            MultisigDomain::recover_multisig_account_by_id(multisig_account_id).await?;
        }

        // update multisig account data
        MultisigAccountDaoV1::update_multisig_address(
            multisig_account_id,
            multisig_account_address,
            salt,
            authority_addr,
            address_type,
            deploy_hash,
            fee_hash,
            fee_chain.clone(),
            pool.as_ref(),
        )
        .await
        .map_err(|e| crate::ServiceError::Database(e.into()))?;

        let account = MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref())
            .await
            .map_err(crate::ServiceError::Database)?;

        if let Some(account) = account {
            // 初始化资产
            crate::domain::assets::AssetsDomain::init_default_multisig_assets(
                multisig_account_address.clone(),
                account.chain_code.clone(),
            )
            .await?;
            // let notification = Notification::new_multisig_notification(
            //     &account.name,
            //     multisig_account_address,
            //     multisig_account_id,
            //     NotificationType::DeployCompletion,
            // );

            // let repo = RepositoryFactory::repo(pool.clone());
            // let system_notification_service = SystemNotificationService::new(repo);

            // system_notification_service
            //     .add_system_notification(msg_id, notification, 0)
            //     .await?;
        }

        let data = NotifyEvent::OrderMultiSignCreated(OrderMultiSignCreatedFrontend {
            multisig_account_id: multisig_account_id.to_string(),
            multisig_account_address: multisig_account_address.to_string(),
            address_type: address_type.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::test::env::get_manager;

    #[tokio::test]
    async fn update_multisig_address() -> anyhow::Result<()> {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await?;

        let pool = crate::Context::get_global_sqlite_pool()?;
        // 准备测试数据
        // let multisig_account_id = uuid::Uuid::new_v4(); // 生成一个新的 UUID 作为测试用的账户 ID
        let multisig_account_id = "216422221999116288";
        let multisig_account_address = "test_multisig_address".to_string();
        let salt = "random_salt".to_string();
        let authority_addr = "我是一个地址".to_string();
        let address_type = 1; // 假设 address_type 是一个整数
        let deploy_hash = "xxx".to_string();
        let fee_hash = "bb".to_string();
        let fee_chain = None;

        wallet_database::dao::multisig_account::MultisigAccountDaoV1::update_multisig_address(
            &multisig_account_id.to_string(),
            &multisig_account_address,
            &salt,
            &authority_addr,
            &address_type.to_string(),
            &deploy_hash,
            &fee_hash,
            fee_chain,
            pool.as_ref(),
        )
        .await?;
        Ok(())
    }
}
