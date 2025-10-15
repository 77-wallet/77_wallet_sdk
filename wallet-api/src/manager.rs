use crate::{
    api::ReturnType,
    context::{Context, init_context},
    data::init_some_data,
    dirs::Dirs,
    domain,
    handles::Handles,
    infrastructure::{self},
    messaging::notify::FrontendNotifyEvent,
    service::{device::DeviceService, task_queue::TaskQueueService},
};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use wallet_database::factory::RepositoryFactory;
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::swap::ApiInitSwapReq;

#[derive(Debug, Clone)]
pub struct WalletManager {
    pub(crate) repo_factory: RepositoryFactory,
    pub(crate) ctx: &'static Context,
    pub(crate) handles: Arc<Handles>,
}

impl WalletManager {
    pub async fn new(
        sn: &str,
        device_type: &str,
        sender: Option<UnboundedSender<FrontendNotifyEvent>>,
        config: crate::config::Config,
        dir: Dirs,
    ) -> Result<WalletManager, crate::error::service::ServiceError> {
        let base_path = infrastructure::log::format::LogBasePath(dir.get_log_dir());
        let context = init_context(sn, device_type, dir, sender, config).await?;
        GLOBAL_KEY.set_sn(sn);

        // 现在的上报日志
        infrastructure::log::start_upload_scheduler(
            base_path,
            5 * 60,
            context.get_global_oss_client(),
        )
        .await?;

        let handles = Arc::new(Handles::new(context.get_client_id()).await);
        handles.get_global_unconfirmed_msg_processor().start().await;
        handles.get_global_task_manager().start_task_check().await?;
        context.set_global_handles(Arc::downgrade(&handles));
        let pool = context.get_global_sqlite_pool()?;
        let repo_factory = RepositoryFactory::new(pool);
        let manager = WalletManager { repo_factory, ctx: context, handles };
        Ok(manager)
    }

    pub async fn init(&self, req: crate::request::devices::InitDeviceReq) -> ReturnType<()> {
        DeviceService::new(self.repo_factory.resource_repo()).init_device(req).await?;
        // TODO ： 某个版本进行取消,
        domain::app::DeviceDomain::check_wallet_password_is_null().await?;

        let backend = self.ctx.get_global_backend_api();
        let req = ApiInitSwapReq {
            sn: self.ctx.get_sn().to_string(),
            client_pub_key: GLOBAL_KEY.secret_pub_key(),
        };
        let res = backend.init_swap(&req).await?;
        if let Some(data) = res.data {
            GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
        }

        tokio::spawn(async move {
            if let Err(e) = init_some_data().await {
                tracing::error!("init_data error: {}", e);
            };
        });

        Ok(())
    }

    pub async fn process_jpush_message(&self, message: &str) -> ReturnType<()> {
        crate::service::jpush::JPushService::jpush(message).await.into()
    }

    pub async fn get_task_queue_status(
        &self,
    ) -> ReturnType<crate::response_vo::task_queue::TaskQueueStatus> {
        TaskQueueService::new(self.repo_factory.resource_repo()).get_task_queue_status().await
    }

    pub async fn set_frontend_notify_sender(
        &self,
        sender: UnboundedSender<FrontendNotifyEvent>,
    ) -> Result<(), crate::error::service::ServiceError> {
        self.ctx.set_frontend_notify_sender(Some(sender)).await
    }

    pub async fn close(&self) -> ReturnType<()> {
        self.handles.close().await.into()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io::Write,
    };
    use tempfile::tempdir;

    use crate::dirs::Dirs;

    #[tokio::test]
    async fn test_traverse_directory_structure() -> Result<(), anyhow::Error> {
        // 创建临时目录结构
        let temp_dir = tempdir()?;
        let root_dir = temp_dir.path();

        // 创建模拟钱包和账户目录结构
        let wallet_a_path = root_dir.join("钱包A");
        let wallet_a_root_path = wallet_a_path.join("root");
        let wallet_a_subs_path = wallet_a_path.join("subs");

        let wallet_b_path = root_dir.join("钱包B");
        let wallet_b_root_path = wallet_b_path.join("root");
        let wallet_b_subs_path = wallet_b_path.join("subs");

        fs::create_dir_all(&wallet_a_root_path)?;
        fs::create_dir_all(&wallet_a_subs_path)?;
        fs::create_dir_all(&wallet_b_root_path)?;
        fs::create_dir_all(&wallet_b_subs_path)?;

        // 创建钱包根密钥文件和种子文件
        let wallet_a_root_pk_file =
            wallet_a_root_path.join("0x296a3C6B001e163409D7df318799bD52B5e3b67d-pk");
        let wallet_a_root_seed_file =
            wallet_a_root_path.join("0x296a3C6B001e163409D7df318799bD52B5e3b67d-seed");
        let wallet_b_root_pk_file =
            wallet_b_root_path.join("0x21A640a53530Aee3feEc2487a01070971d66320f-pk");
        let wallet_b_root_seed_file =
            wallet_b_root_path.join("0x21A640a53530Aee3feEc2487a01070971d66320f-seed");

        File::create(&wallet_a_root_pk_file)?.write_all(b"walletA root pk")?;
        File::create(&wallet_a_root_seed_file)?.write_all(b"walletA root seed")?;
        File::create(&wallet_b_root_pk_file)?.write_all(b"walletB root pk")?;
        File::create(&wallet_b_root_seed_file)?.write_all(b"walletB root seed")?;

        // 创建派生密钥文件
        let wallet_a_sub_key_0 = wallet_a_subs_path.join("address1-m_44'_60'_0'_0_0-pk");
        let wallet_a_sub_key_1 = wallet_a_subs_path.join("address2-m_44'_60'_0'_0_1-pk");
        let wallet_a_sub_key_2 = wallet_a_subs_path.join("address3-m_44'_60'_1'_0_0-pk");

        File::create(&wallet_a_sub_key_0)?.write_all(b"walletA sub key 0")?;
        File::create(&wallet_a_sub_key_1)?.write_all(b"walletA sub key 1")?;
        File::create(&wallet_a_sub_key_2)?.write_all(b"walletA sub key 2")?;

        let dir = &root_dir.to_string_lossy().to_string();
        let dirs = Dirs::new(dir)?;

        let config = crate::config::Config::new(&crate::test::env::get_config()?)?;
        let _manager =
            crate::manager::WalletManager::new("sn", "ANDROID", None, config, dirs).await?;
        let dirs = crate::context::CONTEXT.get().unwrap().get_global_dirs();

        wallet_tree::wallet_hierarchy::v1::LegacyWalletTree::traverse_directory_structure(
            &dirs.wallet_dir,
        )?;

        Ok(())
    }
}
