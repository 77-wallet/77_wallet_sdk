use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use once_cell::sync::Lazy;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tracing::Instrument as _;
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        assets::AssetsId,
        task_queue::{KnownTaskName, TaskName},
    },
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, assets::ApiAssetsRepo, coin::ApiCoinRepo,
            wallet::ApiWalletRepo,
        },
        device::DeviceRepo,
        task_queue::TaskQueueRepo,
        wallet::WalletRepoTrait,
    },
};
use wallet_transport_backend::{
    api::BackendApi,
    consts::endpoint,
    request::{
        ChainRpcListReq, FindConfigByKey,
        api_wallet::address::{AddressListReq, AssetListReq, ExpandAddressCompleteReq},
    },
    response_vo::{app::FindConfigByKeyRes, coin::TokenRates},
};

use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, chain::ApiChainDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        chain::ChainDomain,
        node::NodeDomain,
    },
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        mqtt_api::ApiMqttStruct,
        task::Tasks,
    },
    messaging::{
        mqtt::topics::api_wallet::cmd::address_allock::{AwmCmdAddrExpandMsg, ExpandStatus},
        notify::{FrontendNotifyEvent, api_wallet::AwmCmdAddrExpandMsgFront, event::NotifyEvent},
    },
};
pub struct BackendTaskHandle;

static DEFAULT_ENDPOINTS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        endpoint::multisig::SIGNED_ORDER_ACCEPT,
        endpoint::multisig::SIGNED_ORDER_CANCEL,
        endpoint::multisig::SIGNED_ORDER_UPDATE_RECHARGE_HASH,
        endpoint::multisig::SIGNED_ORDER_UPDATE_SIGNED_HASH,
        endpoint::multisig::SIGNED_TRAN_CREATE,
        endpoint::multisig::SIGNED_TRAN_ACCEPT,
        endpoint::multisig::SIGNED_TRAN_UPDATE_TRANS_HASH,
        endpoint::multisig::SIGNED_ORDER_SAVE_RAW_DATA,
        endpoint::DEVICE_DELETE,
        // endpoint::DEVICE_BIND_ADDRESS,
        endpoint::DEVICE_UNBIND_ADDRESS,
        endpoint::SEND_MSG_CONFIRM,
        endpoint::multisig::PERMISSION_ACCEPT,
        endpoint::UPLOAD_PERMISSION_TRANS,
        endpoint::DEVICE_UPDATE_APP_ID,
        endpoint::KEYS_UPDATE_WALLET_NAME,
        endpoint::ADDRESS_UPDATE_ACCOUNT_NAME,
        endpoint::APP_INSTALL_SAVE,
        endpoint::TOKEN_BALANCE_REFRESH,
        endpoint::SWAP_APPROVE_CANCEL,
        endpoint::SWAP_APPROVE_SAVE,
        // endpoint::api_wallet::ADDRESS_INIT,
    ]
    .iter()
    .cloned()
    .collect()
});

impl BackendTaskHandle {
    pub async fn do_handle(
        endpoint: &str,
        body: serde_json::Value,
        backend: Arc<BackendApi>,
        // wallet_type: WalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        let handler = Self::get_handler(endpoint);
        tracing::info!("endpoint: {endpoint}, body: {body}");
        handler.handle(endpoint, body, backend.as_ref()).await?;

        Ok(())
    }

    pub(crate) fn is_default_endpoint(endpoint: &str) -> bool {
        DEFAULT_ENDPOINTS.contains(&endpoint)
    }

    /// 获取对应的处理策略
    fn get_handler(endpoint: &str) -> Box<dyn EndpointHandler + Send + Sync> {
        if Self::is_default_endpoint(endpoint) {
            Box::new(DefaultHandler)
        } else {
            Box::new(SpecialHandler)
        }
    }
}

/// 定义一个处理策略的 trait
#[async_trait::async_trait]
trait EndpointHandler {
    async fn handle(
        &self,
        endpoint: &str,
        body: serde_json::Value,
        backend: &BackendApi,
        // wallet_type: WalletType,
    ) -> Result<(), crate::error::service::ServiceError>;
}

/// 默认的处理策略
struct DefaultHandler;

#[async_trait::async_trait]
impl EndpointHandler for DefaultHandler {
    async fn handle(
        &self,
        endpoint: &str,
        body: serde_json::Value,
        backend: &BackendApi,
        // _wallet_type: WalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        let Some(device) = DeviceRepo::get_device_info(pool, sn).await? else {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        };

        if device.is_init != 1 {
            return Err(crate::error::business::BusinessError::Device(
                crate::error::business::device::DeviceError::Uninitialized,
            )
            .into());
        }
        // let invite_code = ConfigDomain::get_invite_code().await?;
        // if invite_code.status.is_none() {
        //     return Err(
        //         crate::BusinessError::Device(crate::DeviceError::InviteStatusNotConfirmed).into(),
        //     );
        // }
        // 实现具体的处理逻辑
        let _res = backend.post_req_str::<serde_json::Value>(endpoint, &body).await?;
        Ok(())
    }
}

/// 特殊的处理策略
struct SpecialHandler;

#[async_trait]
impl EndpointHandler for SpecialHandler {
    async fn handle(
        &self,
        endpoint: &str,
        body: serde_json::Value,
        backend: &BackendApi,
        // TODO： 完全不需要这个
        // wallet_type: WalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let sn = crate::context::CONTEXT.get().unwrap().get_sn();
        match endpoint {
            endpoint::DEVICE_INIT => {
                let res = backend.post_req_str::<Option<()>>(endpoint, &body).await;
                res?;
                use wallet_database::repositories::device::DeviceRepoTrait as _;
                repo.device_init(sn).await?;
            }
            endpoint::KEYS_V2_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(crate::error::business::BusinessError::Config(
                        crate::error::business::config::ConfigError::KeysNotReset,
                    )
                    .into());
                }

                let res = backend.post_req_str::<Option<()>>(endpoint, &body).await;

                // TODO 先单独在这里发送事件，后期修改为统一处理
                #[cfg(not(feature = "prod"))]
                if let Err(ref e) = res {
                    let message = serde_json::json!({
                        "event": "keys_init_fail",
                        "message": e.to_string(),
                    });
                    let _r = FrontendNotifyEvent::send_debug(message).await;
                }
                res?;

                let req: wallet_transport_backend::request::KeysInitReq =
                    wallet_utils::serde_func::serde_from_value(body)?;
                use wallet_database::repositories::wallet::WalletRepoTrait as _;
                repo.wallet_init(&req.uid).await?;
            }
            endpoint::old_wallet::OLD_KEYS_V2_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(crate::error::business::BusinessError::Config(
                        crate::error::business::config::ConfigError::KeysNotReset,
                    )
                    .into());
                }

                let res = backend.post_req_str::<Option<()>>(endpoint, &body).await;

                // TODO 先单独在这里发送事件，后期修改为统一处理
                #[cfg(not(feature = "prod"))]
                if let Err(ref e) = res {
                    let message = serde_json::json!({
                        "event": "keys_init_fail",
                        "message": e.to_string(),
                    });
                    let _r = FrontendNotifyEvent::send_debug(message).await;
                }
                res?;
                let req: wallet_transport_backend::request::KeysInitReq =
                    wallet_utils::serde_func::serde_from_value(body)?;
                ApiWalletRepo::mark_init(&pool, &req.uid).await?;
            }

            endpoint::api_wallet::ADDRESS_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(crate::error::business::BusinessError::Config(
                        crate::error::business::config::ConfigError::KeysNotReset,
                    )
                    .into());
                }

                let req: wallet_transport_backend::request::api_wallet::address::ApiAddressInitReq =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;

                let mut indices: HashMap<String, Vec<i32>> = HashMap::new();
                for address in req.address_list.0.iter() {
                    let wallet = ApiWalletRepo::find_by_uid(&pool, &address.uid).await?;

                    match wallet {
                        Some(wallet) => {
                            if wallet.is_init == 1 {
                                ApiAccountRepo::init(&pool, &address.address, &address.chain_code)
                                    .await?;
                                indices
                                    .entry(address.uid.clone())
                                    .and_modify(|v| v.push(address.index))
                                    .or_insert(vec![address.index]);
                                continue;
                            } else {
                                return Err(crate::error::business::BusinessError::ApiWallet(
                                    crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                                )
                                .into());
                            }
                        }
                        None => {
                            return Err(crate::error::business::BusinessError::ApiWallet(
                                crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                            )
                            .into());
                        }
                    }
                }

                tracing::info!("ADDRESS_INIT -------------- 1");
                backend.post_req_str::<()>(endpoint, &body).await?;
                tracing::info!("ADDRESS_INIT -------------- 2");
                // backend.expand_address(&req).await?;

                let task = TaskQueueRepo::get_task_with_task_name(
                    &pool,
                    TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
                    &[0, 1, 3],
                )
                .await?;
                tracing::info!("ADDRESS_INIT -------------- 3");
                tracing::info!("ADDRESS_INIT -------------- task: {:?}", task);

                if let Some(task) = task
                    && let Some(reamrk) = task.remark
                {
                    tracing::info!("ADDRESS_INIT -------------- 4");
                    let mut remark =
                        wallet_utils::serde_func::serde_from_str::<ExpandStatus>(&reamrk)?;
                    tracing::info!("ADDRESS_INIT -------------- 5");

                    let msg: ApiMqttStruct =
                        wallet_utils::serde_func::serde_from_str(&task.request_body)?;
                    let msg = wallet_utils::serde_func::serde_from_value::<AwmCmdAddrExpandMsg>(
                        msg.data,
                    )?;

                    tracing::info!("ADDRESS_INIT -------------- 6");
                    tracing::info!("ADDRESS_INIT indices: {:?}", indices);
                    for (uid, idx) in &indices {
                        if *uid == msg.uid {
                            for id in idx {
                                if remark.needed_indices.contains(id)
                                    && !remark.completed_indices.contains(id)
                                {
                                    remark.completed_indices.insert(*id);
                                }
                            }
                        }
                    }

                    let all_done =
                        remark.needed_indices.iter().all(|i| remark.completed_indices.contains(i));
                    if all_done {
                        remark.status = true; // 标记为完成
                        tracing::info!(
                            "ADDRESS_INIT 扩容任务全部完成，needed_indices = {:?}, completed_indices = {:?}",
                            remark.needed_indices,
                            remark.completed_indices
                        );
                    } else {
                        remark.status = false;
                        tracing::info!(
                            "ADDRESS_INIT 部分完成，needed_indices = {:?}, completed_indices = {:?}",
                            remark.needed_indices,
                            remark.completed_indices
                        );
                    }

                    let updated_remark = wallet_utils::serde_func::serde_to_string(&remark)?;
                    TaskQueueRepo::update_task_remark(&pool, &task.id, &updated_remark).await?;

                    if remark.status {
                        let req =
                            ExpandAddressCompleteReq::new(&msg.uid, &msg.serial_no, true, None);
                        backend.expand_address_complete(req).await?;
                    }
                }
            }
            endpoint::old_wallet::OLD_ADDRESS_BATCH_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(crate::error::business::BusinessError::Config(
                        crate::error::business::config::ConfigError::KeysNotReset,
                    )
                    .into());
                }

                let req: wallet_transport_backend::request::AddressBatchInitReq =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;

                for address in req.0 {
                    let wallet = ApiWalletRepo::find_by_uid(&pool, &address.uid).await?;

                    match wallet {
                        Some(wallet) => {
                            if wallet.is_init == 1 {
                                ApiAccountRepo::init(&pool, &address.address, &address.chain_code)
                                    .await?;
                                continue;
                            } else {
                                return Err(crate::error::business::BusinessError::ApiWallet(
                                    crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                                )
                                .into());
                            }
                        }
                        None => {
                            return Err(crate::error::business::BusinessError::ApiWallet(
                                crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                            )
                            .into());
                        }
                    }
                }

                let res = backend.post_req_str::<()>(endpoint, &body).await;
                res?;
            }

            endpoint::DEVICE_EDIT_DEVICE_INVITEE_STATUS => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                let Some(device) = DeviceRepo::get_device_info(pool, sn).await? else {
                    return Err(crate::error::business::BusinessError::Device(
                        crate::error::business::device::DeviceError::Uninitialized,
                    )
                    .into());
                };

                if device.is_init != 1 {
                    return Err(crate::error::business::BusinessError::Device(
                        crate::error::business::device::DeviceError::Uninitialized,
                    )
                    .into());
                }

                let req: wallet_transport_backend::request::SetInviteeStatusReq =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;
                let res = backend.post_req_str::<Option<()>>(endpoint, &body).await;

                res?;
                let code = ConfigDomain::get_invite_code().await?.and_then(|c| c.code);

                ConfigDomain::set_invite_code(Some(req.invitee), code).await?;
            }
            endpoint::LANGUAGE_INIT => {
                backend.post_req_str::<()>(endpoint, &body).await?;
                use wallet_database::repositories::device::DeviceRepoTrait as _;
                repo.language_init(sn).await?;
                let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
                crate::domain::announcement::AnnouncementDomain::pull_announcement(&mut repo)
                    .await?;
            }
            endpoint::ADDRESS_BATCH_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(crate::error::business::BusinessError::Config(
                        crate::error::business::config::ConfigError::KeysNotReset,
                    )
                    .into());
                }

                let req: wallet_transport_backend::request::AddressBatchInitReq =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;

                for address in req.0 {
                    let wallet = repo.wallet_detail_by_uid(&address.uid).await?;

                    match wallet {
                        Some(wallet) => {
                            if wallet.is_init == 1 {
                                use wallet_database::repositories::account::AccountRepoTrait as _;
                                repo.account_init(&address.address, &address.chain_code).await?;
                                continue;
                            } else {
                                return Err(crate::error::business::BusinessError::Wallet(
                                    crate::error::business::wallet::WalletError::NotInit,
                                )
                                .into());
                            }
                        }
                        None => {
                            return Err(crate::error::business::BusinessError::Wallet(
                                crate::error::business::wallet::WalletError::NotFound,
                            )
                            .into());
                        }
                    }
                }

                let res = backend.post_req_str::<()>(endpoint, &body).await;
                res?;
            }
            endpoint::TOKEN_CUSTOM_TOKEN_INIT => {
                let res = backend.post_req_str::<bool>(endpoint, &body).await;
                res?;

                let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
                let coin_service = crate::service::coin::CoinService::new(repo);
                coin_service.init_token_price().await?;
            }

            endpoint::TOKEN_QUERY_RATES => {
                let rates: TokenRates = backend.post_req_str::<TokenRates>(endpoint, &body).await?;

                let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
                let exchange_rate_service =
                    crate::service::exchange_rate::ExchangeRateService::new(repo);
                exchange_rate_service.init(rates).await?;
            }
            endpoint::SYS_CONFIG_FIND_CONFIG_BY_KEY => {
                let req: FindConfigByKey =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;
                match req.key.as_str() {
                    "OFFICIAL:WEBSITE" => {
                        let res =
                            backend.post_req_str::<FindConfigByKeyRes>(endpoint, &body).await?;
                        ConfigDomain::set_official_website(res.value).await?;
                    }
                    _ => {
                        tracing::warn!("unknown key: {}", req.key);
                    }
                }
            }
            endpoint::APP_INSTALL_DOWNLOAD => {
                let url = backend.post_req_str::<String>(endpoint, &body).await?;
                ConfigDomain::set_app_download_qr_code_url(&url).await?;
                // ConfigDomain::set_version_download_url(&url).await?;
            }
            endpoint::VERSION_VIEW => {
                // let app_version_res = backend
                //     .post_req_str::<wallet_transport_backend::response_vo::app::AppVersionRes>(
                //         endpoint, &body,
                //     )
                //     .await?;
                // ConfigDomain::set_version_download_url(app_version_res.download_url)
            }
            endpoint::CHAIN_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainList>(
                        endpoint, &body,
                    )
                    .await?;

                //先插入再过滤
                ChainDomain::upsert_multi_chain_than_toggle(input).await?;
            }
            endpoint::api_wallet::API_WALLET_CHAIN_LIST => {
                let body: HashMap<String, String> =
                    wallet_utils::serde_func::serde_from_value(body)?;
                let app_version_code = body.get("appVersionCode");
                let input = backend.api_wallet_chain_list(app_version_code.unwrap()).await?;
                //先插入再过滤
                ApiChainDomain::upsert_multi_api_chain_than_toggle(input).await?;
            }
            endpoint::CHAIN_RPC_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainInfos>(
                        endpoint, &body,
                    )
                    .await?;
                let req = wallet_utils::serde_func::serde_from_value::<ChainRpcListReq>(body)?;
                let mut backend_nodes = Vec::new();
                NodeDomain::upsert_chain_rpc(&mut repo, input, &mut backend_nodes).await?;
                ChainDomain::sync_nodes_and_link_to_chains(
                    &mut repo,
                    &req.chain_code,
                    &backend_nodes,
                )
                .await?;
                ApiChainDomain::sync_nodes_and_link_to_api_chains(
                    &mut repo,
                    &req.chain_code,
                    &backend_nodes,
                )
                .await?;
            }
            endpoint::old_wallet::OLD_CHAIN_RPC_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainInfos>(
                        endpoint, &body,
                    )
                    .await?;
                let req = wallet_utils::serde_func::serde_from_value::<ChainRpcListReq>(body)?;
                let mut backend_nodes = Vec::new();
                NodeDomain::upsert_chain_rpc(&mut repo, input, &mut backend_nodes).await?;
                ApiChainDomain::sync_nodes_and_link_to_api_chains(
                    &mut repo,
                    &req.chain_code,
                    &backend_nodes,
                )
                .await?;
            }
            endpoint::MQTT_INIT => {
                // 1.4 version 注释掉,
                // let mqtt_url = backend
                //     .post_req_str::<String>(endpoint, &body)
                //     .await?;
                // ConfigDomain::set_mqtt_url(Some(mqtt_url)).await?;
            }
            endpoint::KEYS_RESET => {
                match backend.post_req_str::<Option<()>>(endpoint, &body).await {
                    Ok(_) => {}
                    Err(err) => {
                        ConfigDomain::set_keys_reset_status(Some(false)).await?;
                        return Err(err.into());
                    }
                };
                ConfigDomain::set_keys_reset_status(Some(true)).await?;
            }
            endpoint::api_wallet::QUERY_ADDRESS_LIST => {
                let req =
                    wallet_utils::serde_func::serde_from_value::<AddressListReq>(body.clone())?;
                let status = ApiWalletDomain::query_uid_bind_info(&req.uid).await?;

                if !status.bind_status {
                    tracing::warn!("this wallet was not binded");
                    return Ok(());
                }
                let res = backend.query_used_address_list(&req).await?;
                let list = res.content;
                tracing::debug!("query_used_address_list req: {:?}", req);
                tracing::debug!("query_used_address_list list: {:?}", list);
                const BATCH_SIZE: usize = 10;
                let password = ApiWalletDomain::get_passwd().await?;

                let mut all_input_indices = Vec::new();
                let len = list.len();

                let mut done = 0;
                for (i, address) in list.into_iter().enumerate() {
                    all_input_indices.push(address.index);

                    if all_input_indices.len() >= BATCH_SIZE || i == len - 1 {
                        let batch_indices = all_input_indices.clone();
                        all_input_indices.clear(); // 下一批

                        // 批量创建账户
                        if let Some(wallet) = ApiWalletRepo::find_by_uid(&pool, &req.uid).await? {
                            ApiAccountDomain::create_api_account(
                                &wallet.address,
                                &password,
                                vec![req.chain_code.clone()],
                                &batch_indices,
                                "账户",
                                true,
                                wallet.api_wallet_type,
                            )
                            .await?;

                            done += batch_indices.len();
                            let partial_notify =
                                NotifyEvent::AddressRecovery(AwmCmdAddrExpandMsgFront {
                                    uid: req.uid.to_string(),
                                    done_number: done as u32,
                                    number: res.total_elements as u32,
                                });
                            if let Err(e) = FrontendNotifyEvent::new(partial_notify).send().await {
                                tracing::warn!("Failed to send partial notify: {}", e);
                            }
                        }

                        // 创建余额查询任务
                        let asset_list_req =
                            AssetListReq::new(&req.uid, &req.chain_code, batch_indices);
                        let asset_list_task_data = BackendApiTaskData::new(
                            wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ASSET_LIST,
                            &asset_list_req,
                        )?;

                        Tasks::new()
                            .push(BackendApiTask::BackendApi(asset_list_task_data))
                            .send()
                            .await?;

                        tracing::debug!(
                            "Dispatched asset query for a batch of {} addresses",
                            BATCH_SIZE
                        );
                    }
                }

                // if !input_indices.is_empty() {
                //     let asset_list_req =
                //         AssetListReq::new(&req.uid, &req.chain_code, input_indices.clone());
                //     let asset_list_task_data = BackendApiTaskData::new(
                //         wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ASSET_LIST,
                //         &asset_list_req,
                //     )?;
                //     tasks = tasks.push(BackendApiTask::BackendApi(asset_list_task_data));
                // }

                // let password = ApiWalletDomain::get_passwd().await?;
                // if let Some(wallet) = ApiWalletRepo::find_by_uid(&pool, &req.uid).await? {
                //     ApiAccountDomain::create_api_account(
                //         &wallet.address,
                //         &password,
                //         vec![req.chain_code.clone()],
                //         input_indices,
                //         "账户",
                //         true,
                //         wallet.api_wallet_type,
                //     )
                //     .await?;
                // }

                // tasks.send().await?;
                tracing::info!("QUERY_ADDRESS_LIST -------------------- 4");
                if !res.last {
                    let page = res.number + 1;
                    let query_address_list_req =
                        AddressListReq::new(&req.uid, &req.chain_code, page, 100);

                    let query_address_list_task_data = BackendApiTaskData::new(
                        wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
                        &query_address_list_req,
                    )?;
                    Tasks::new()
                        .push(BackendApiTask::BackendApi(query_address_list_task_data))
                        .send()
                        .await?;
                }
            }
            endpoint::api_wallet::QUERY_ASSET_LIST => {
                let req = wallet_utils::serde_func::serde_from_value::<AssetListReq>(body.clone())?;
                let list = backend.query_asset_list(&req).await?;
                // let list = backend.post_req_str::<serde_json::Value>(endpoint, &body).await?;
                let default_coins_list = ApiCoinRepo::coin_list(&pool).await?;

                tracing::debug!("QUERY_ASSET_LIST -------------------- 1 list: {list:?}");
                tracing::debug!(
                    "QUERY_ASSET_LIST -------------------- 1 default_coins_list: {default_coins_list:?}"
                );
                let mut tasks = Vec::new();
                for asset in list.0 {
                    for address in asset.address_list {
                        for token in address.token_infos {
                            if let Some(coin) = default_coins_list.iter().find(|coin| {
                                coin.chain_code == req.chain_code
                                    && coin.token_address.as_ref() == Some(&token.token_address)
                            }) {
                                let address_clone = address.address.clone();
                                tasks.push((address_clone, token, coin));
                            }
                        }
                    }
                }
                let total_tasks = tasks.len();
                // 全局计数器：统计已处理的 asset 数量（用于验证子任务确实被执行）
                let processed = Arc::new(AtomicUsize::new(0));

                const BATCH_SIZE: usize = 10;

                tracing::debug!("DEBUG: total tasks = {}", tasks.len());
                for (batch_idx, chunk) in tasks.chunks(BATCH_SIZE).enumerate() {
                    let chunk_len = chunk.len();
                    tracing::debug!("Starting batch {} ({} items)", batch_idx + 1, chunk_len);

                    // 把这一批克隆成 owned vec（避免借用/生命周期问题）
                    let chunk_vec: Vec<_> = chunk.to_vec();

                    // 克隆必要的环境变量到闭包
                    let pool_for_tasks = pool.clone();
                    let chain_code_for_tasks = req.chain_code.clone();
                    let processed_for_tasks = processed.clone();

                    stream::iter(chunk_vec.into_iter())
                        .for_each_concurrent(10, move |(address, token, coin)| {
                            // 为每个任务创建 span，保证 tracing context 被传递
                            let span = tracing::info_span!("asset_process", address = %address, batch = batch_idx + 1);
                            let pool = pool_for_tasks.clone();
                            let chain_code = chain_code_for_tasks.clone();
                            let processed = processed_for_tasks.clone();

                            async move {
                                tracing::debug!("processing asset {}", address);

                                let assets_id = AssetsId::new(
                                    &address,
                                    &chain_code,
                                    &token.symbol,
                                    Some(token.token_address.clone()),
                                );

                                let assets = ApiCreateAssetsVo::new(
                                    assets_id,
                                    coin.decimals,
                                    coin.protocol.clone(),
                                    0,
                                )
                                .with_name(&coin.name)
                                .with_u256(alloy::primitives::U256::default(), coin.decimals)
                                .unwrap_or_default();

                                if let Err(e) = ApiAssetsRepo::upsert_assets(&pool, assets).await {
                                    tracing::error!("upsert_assets failed for {}: {}", address, e);
                                    return;
                                }

                                if let Err(e) = ApiAssetsRepo::update_balance(
                                    &pool,
                                    &address,
                                    &chain_code,
                                    Some(token.token_address.clone()),
                                    &token.amount.to_string(),
                                )
                                .await
                                {
                                    tracing::error!("update_balance failed for {}: {}", address, e);
                                    return;
                                }

                                let Ok(account) = ApiAccountRepo::find_one_by_address(&address, &pool).await else {
                                    tracing::error!("find_one_by_address failed for {}", address);
                                    return;
                                };

                                if let Some(account) = account {
                                    crate::infrastructure::asset_calc::on_asset_update(
                                        &account.wallet_address,
                                        &address,
                                        &chain_code,
                                        &token.token_address,
                                    );
                                }

                                // 增加计数，便于外部核对
                                let prev = processed.fetch_add(1, Ordering::SeqCst);
                                tracing::debug!("TASK_DONE address={} batch={} processed_count={}", address, batch_idx + 1, prev + 1);
                                tracing::debug!("finished asset {}", address);
                            }
                            .instrument(span)
                        })
                        .await;

                    // 每批完成后发送带 batch 信息的通知（确保唯一）
                    let total_batches = (total_tasks + BATCH_SIZE - 1) / BATCH_SIZE;
                    // 打印并发送通知
                    tracing::info!(
                        "SENDING_PARTIAL_NOTIFY batch={}/{} processed_so_far={}",
                        batch_idx + 1,
                        total_batches,
                        processed.load(Ordering::SeqCst)
                    );
                }
            }
            _ => {
                // 未知的 endpoint
                tracing::warn!("unknown endpoint: {}", endpoint);
                Err(crate::error::service::ServiceError::System(
                    crate::error::system::SystemError::BackendEndpointNotFound,
                ))?;
            }
        }

        Ok(())
    }
}
