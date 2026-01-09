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
use tokio::sync::Mutex;
use wallet_database::{
    entities::{
        address_query_state::{AddressQueryStatus, CreateAddressQueryStateEntity},
        api_assets::ApiCreateAssetsVo,
        assets::AssetsId,
    },
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, address_query_state::AddressQueryStateRepo,
            assets::ApiAssetsRepo, coin::ApiCoinRepo, wallet::ApiWalletRepo,
        },
        device::DeviceRepo,
        wallet::WalletRepoTrait,
    },
};
use wallet_transport_backend::{
    api::BackendApi,
    consts::endpoint,
    request::{
        FindConfigByKey,
        api_wallet::address::{AddressListReq, AssetListReq},
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
    infrastructure::{
        asset_calc::actor_model::AssetKey,
        chain_node::chain_node_ensurer::ChainNodeEnsurer,
        expand_address::facade::ExpandAddressFacade,
        task_queue::{
            backend::{BackendApiTask, BackendApiTaskData},
            task::Tasks,
        },
    },
    messaging::notify::{
        FrontendNotifyEvent, api_wallet::AwmCmdAddrExpandMsgFront, event::NotifyEvent,
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
        // endpoint::DEVICE_DELETE,
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
        tracing::debug!("endpoint: {endpoint}, body: {body}");
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
        let _res = backend.post_default(endpoint, &body).await?;
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

                tracing::debug!("开始处理地址初始化请求: {:?}", body);
                let req: wallet_transport_backend::request::api_wallet::address::ApiAddressInitReq =
                    wallet_utils::serde_func::serde_from_value(body.clone())?;

                tracing::debug!(
                    "开始处理地址初始化请求: 请求地址数量={}, batch_id={:?} 重置状态检查通过",
                    req.address_list.0.len(),
                    req.batch_id
                );

                backend.expand_address(&req).await?;
                tracing::debug!("后端地址扩容调用完成: 请求地址数量={}", req.address_list.0.len());

                let mut indices_by_uid: HashMap<(String, String), Vec<i32>> = HashMap::new();
                tracing::debug!("开始处理地址初始化数据库操作");

                for address in req.address_list.0.iter() {
                    tracing::debug!(
                        "处理地址: uid={}, chain_code={}, index={}, address={}",
                        address.uid,
                        address.chain_code,
                        address.index,
                        address.address
                    );

                    let wallet = ApiWalletRepo::find_by_uid(pool.clone(), &address.uid).await?;

                    match wallet {
                        Some(wallet) => {
                            if wallet.is_init == 1 {
                                ApiAccountRepo::init(
                                    pool.clone(),
                                    &address.address,
                                    &address.chain_code,
                                )
                                .await?;
                                indices_by_uid
                                    .entry((address.uid.clone(), address.chain_code.clone()))
                                    .and_modify(|v| v.push(address.index))
                                    .or_insert(vec![address.index]);
                                continue;
                            } else {
                                tracing::warn!("钱包未初始化: uid={}", address.uid);
                                return Err(crate::error::business::BusinessError::ApiWallet(
                                    crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                                )
                                .into());
                            }
                        }
                        None => {
                            tracing::warn!("钱包不存在: uid={}", address.uid);
                            return Err(crate::error::business::BusinessError::ApiWallet(
                                crate::error::business::api_wallet::ApiWalletError::WalletNotInit,
                            )
                            .into());
                        }
                    }
                }

                tracing::debug!(
                    "数据库初始化操作完成，准备通知Actor: 处理UID数量={}",
                    indices_by_uid.len()
                );

                // 使用Actor模型处理地址初始化通知
                if let Some(batch_id) = req.batch_id.as_deref() {
                    for ((uid, chain_code), indices) in indices_by_uid {
                        tracing::debug!(
                            "处理UID地址初始化通知: uid={}, chain_code={}, 索引数量={}, 索引列表={:?}",
                            uid,
                            chain_code,
                            indices.len(),
                            indices
                        );

                        // 通知Actor地址已初始化（批量处理）
                        tracing::debug!(
                            "提交地址初始化通知: uid={}, chain_code={}, indices={:?}",
                            uid,
                            chain_code,
                            indices,
                        );

                        // 移除对ExpandAddressFacade::submit_address_inited的调用
                        // Scanner会定期扫描并推进状态，不依赖外部通知
                        tracing::debug!(
                            "地址初始化完成，Scanner将定期扫描并推进状态: uid={}, chain_code={}, indices={:?}",
                            uid,
                            chain_code,
                            indices
                        );
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
                    let wallet = ApiWalletRepo::find_by_uid(pool.clone(), &address.uid).await?;

                    match wallet {
                        Some(wallet) => {
                            if wallet.is_init == 1 {
                                ApiAccountRepo::init(
                                    pool.clone(),
                                    &address.address,
                                    &address.chain_code,
                                )
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
                // 1. 后端 chains → upsert 到本地
                ChainDomain::init_load_backend_chains(input).await?;
                // 2. 基于本地 chains → 触发去拉 nodes
                NodeDomain::init_sync_nodes().await?;
                // 3. 兜底保证每条链都有 node
                let ensurer = ChainNodeEnsurer::new(pool.clone());
                ensurer.ensure_all().await?;
            }
            endpoint::api_wallet::API_WALLET_CHAIN_LIST => {
                let body: HashMap<String, String> =
                    wallet_utils::serde_func::serde_from_value(body)?;
                let app_version_code = body.get("appVersionCode");
                let input = backend.api_wallet_chain_list(app_version_code.unwrap()).await?;
                //先插入再过滤
                if !ApiChainDomain::upsert_multi_api_chain_than_toggle(input).await?.is_empty() {
                    let password = ApiWalletDomain::get_passwd().await?;
                    ApiChainDomain::sync_wallet_chain_data(&password).await?;
                }
            }
            endpoint::CHAIN_RPC_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainInfos>(
                        endpoint, &body,
                    )
                    .await?;
                NodeDomain::upsert_chain_rpc(&mut repo, input).await?;
                let ensurer = ChainNodeEnsurer::new(pool.clone());
                ensurer.ensure_all().await?;
            }
            endpoint::old_wallet::OLD_CHAIN_RPC_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainInfos>(
                        endpoint, &body,
                    )
                    .await?;
                NodeDomain::upsert_chain_rpc(&mut repo, input).await?;
                let ensurer = ChainNodeEnsurer::new(pool.clone());
                ensurer.ensure_all().await?;
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
                use std::time::Instant;
                let start_total = Instant::now();

                // 1. 反序列化请求体
                let start_deserialize = Instant::now();
                let mut req =
                    wallet_utils::serde_func::serde_from_value::<AddressListReq>(body.clone())?;
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST start: uid={}, chain_code={}",
                    req.uid,
                    req.chain_code
                );
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: deserialized request in {:?}, uid={}, chain_code={}",
                    start_deserialize.elapsed(),
                    req.uid,
                    req.chain_code
                );

                // 2. 查询地址查询状态，决定起始页码
                let start_check_state = Instant::now();
                let state =
                    AddressQueryStateRepo::get_by_uid_and_chain(&pool, &req.uid, &req.chain_code)
                        .await?;

                let start_page = match state {
                    None => {
                        // 第一次查询，插入状态
                        let query_state = CreateAddressQueryStateEntity::new(
                            &req.uid,
                            &req.chain_code,
                            AddressQueryStatus::Running,
                        );
                        AddressQueryStateRepo::upsert(&pool, query_state).await?;
                        0
                    }
                    Some(s) if s.status == AddressQueryStatus::Done => {
                        // 已经完成，直接返回
                        tracing::debug!(
                            "Address query already done for uid={}, chain_code={}",
                            req.uid,
                            req.chain_code
                        );
                        return Ok(());
                    }
                    Some(s) => {
                        // 从上次完成的页码 + 1 开始
                        s.last_page + 1
                    }
                };

                // 设置请求页码
                req.page_num = start_page as i32;
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: checked query state in {:?}, uid={}, chain_code={}, start_page={}",
                    start_check_state.elapsed(),
                    req.uid,
                    req.chain_code,
                    start_page
                );

                // 4. 调用后端查询地址列表
                let start_backend_query = Instant::now();
                let res = backend.query_used_address_list(&req).await?;
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: backend query in {:?}, uid={}, chain_code={}, total_elements={}",
                    start_backend_query.elapsed(),
                    req.uid,
                    req.chain_code,
                    res.total_elements
                );

                let list = res.content;
                tracing::debug!("query_used_address_list req: {:?}", req);
                tracing::debug!("query_used_address_list list: {:?}", list);

                let mut done = 0;
                tracing::debug!("查询地址列表： total_elements: {}", res.total_elements);

                // 5. 处理地址列表
                let start_batch_process = Instant::now();

                // 5.1 获取后端地址索引集合
                let backend_indices: Vec<i32> = list.iter().map(|addr| addr.index).collect();
                let backend_indices_set: std::collections::HashSet<i32> =
                    backend_indices.iter().cloned().collect();

                // 5.2 查询本地数据库中已存在的地址索引
                let wallet = ApiWalletRepo::find_by_uid(pool.clone(), &req.uid).await?.ok_or(
                    crate::error::business::BusinessError::ApiWallet(
                        crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
                    ),
                )?;

                let local_indices_tuples = ApiAccountRepo::list_inited_indices(
                    pool.clone(),
                    &wallet.address,
                    &req.chain_code,
                )
                .await?;
                let local_indices: Vec<i32> =
                    local_indices_tuples.iter().map(|(idx,)| *idx).collect();
                let local_indices_set: std::collections::HashSet<i32> =
                    local_indices.iter().cloned().collect();

                // 5.3 计算需要恢复的地址（后端有但本地没有）
                let need_recover: Vec<i32> =
                    backend_indices_set.difference(&local_indices_set).cloned().collect();

                tracing::debug!(
                    "地址处理分析：后端索引数={}, 本地索引数={}, 需要恢复={}",
                    backend_indices.len(),
                    local_indices.len(),
                    need_recover.len()
                );

                // 5.5 批量恢复已有地址（直接插入，不调用create）
                if !need_recover.is_empty() {
                    let start_recover = Instant::now();

                    // 获取需要恢复的地址详情
                    let addresses_to_recover: Vec<_> = list
                        .into_iter()
                        .filter(|addr| need_recover.contains(&addr.index))
                        .collect();

                    // 批量恢复已有地址，使用优化后的create_api_account
                    // 提取需要恢复的索引列表
                    let input_indices: Vec<_> =
                        addresses_to_recover.iter().map(|addr| addr.index).collect();

                    // 调用优化后的create_api_account，使用快速路径+慢速路径模式
                    ApiAccountDomain::create_api_account(
                        &wallet.address,
                        vec![req.chain_code.to_string()],
                        &input_indices,
                        "账户",
                        true, // is_default_name
                        wallet.api_wallet_type,
                        None, // batch_id
                        true, // is_recover - 恢复模式
                    )
                    .await?;

                    done += addresses_to_recover.len();
                    tracing::debug!(
                        "[PERF] QUERY_ADDRESS_LIST: recovered {} addresses in {:?}, uid={}, chain_code={}",
                        addresses_to_recover.len(),
                        start_recover.elapsed(),
                        req.uid,
                        req.chain_code
                    );
                }

                // 5.7 发送最终通知
                let start_send_notify = Instant::now();
                let final_notify = NotifyEvent::AddressRecovery(AwmCmdAddrExpandMsgFront {
                    uid: req.uid.to_string(),
                    done_number: done as u32,
                    number: res.total_elements as u32,
                });
                if let Err(e) = FrontendNotifyEvent::new(final_notify).send().await {
                    tracing::warn!("Failed to send final notify: {}", e);
                }
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: sent final notify in {:?}, uid={}, done={}, total={}",
                    start_send_notify.elapsed(),
                    req.uid,
                    done,
                    res.total_elements
                );

                // 5.8 创建余额查询请求，直接调用，不使用 tasks 系统
                let start_asset_task = Instant::now();

                // 创建余额查询请求，只使用 need_recover
                let all_new_count = need_recover.len();

                if all_new_count > 0 {
                    let all_new_indices = need_recover.clone();
                    let asset_list_req =
                        AssetListReq::new(&req.uid, &req.chain_code, all_new_indices);
                    let asset_list_task_data = BackendApiTaskData::new(
                        wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ASSET_LIST,
                        &asset_list_req,
                    )?;

                    Tasks::new()
                        .push(BackendApiTask::BackendApi(asset_list_task_data))
                        .send()
                        .await?;
                    tracing::debug!(
                        "[PERF] QUERY_ADDRESS_LIST: processed asset query for {} addresses in {:?}, uid={}, chain_code={}",
                        all_new_count,
                        start_asset_task.elapsed(),
                        req.uid,
                        req.chain_code
                    );
                }

                tracing::debug!("查询地址列表：处理完成，共恢复 {} 个地址", need_recover.len());
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: batch processing completed in {:?}, uid={}, chain_code={}, done={}",
                    start_batch_process.elapsed(),
                    req.uid,
                    req.chain_code,
                    done
                );

                tracing::debug!("查询地址列表： create_api_account done: done: {done}");

                // 9. 更新进度
                let start_update_progress = Instant::now();
                if !res.last {
                    // 不是最后一页，更新 last_page
                    AddressQueryStateRepo::update_last_page(
                        &pool,
                        &req.uid,
                        &req.chain_code,
                        res.number.into(),
                        res.total_elements as i64,
                    )
                    .await?;

                    // 直接递归调用下一页，不需要 tasks 系统
                    let next_page = res.number + 1;
                    let query_address_list_req =
                        AddressListReq::new(&req.uid, &req.chain_code, next_page, 1000);
                    let query_address_list_body = serde_json::to_value(query_address_list_req)
                        .map_err(|e| {
                            crate::error::service::ServiceError::System(
                                crate::error::system::SystemError::Internal(e.to_string()),
                            )
                        })?;

                    tracing::debug!(
                        "[PERF] QUERY_ADDRESS_LIST: updating progress in {:?}, uid={}, chain_code={}, next_page={}",
                        start_update_progress.elapsed(),
                        req.uid,
                        req.chain_code,
                        next_page
                    );

                    // 直接调用下一页，实现断点续查
                    let query_address_list_task_data = BackendApiTaskData::new(
                        wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
                        &query_address_list_body,
                    )?;

                    Tasks::new()
                        .push(BackendApiTask::BackendApi(query_address_list_task_data))
                        .send()
                        .await?;
                } else {
                    // 最后一页，更新状态为 Done
                    AddressQueryStateRepo::update_status(
                        &pool,
                        &req.uid,
                        &req.chain_code,
                        AddressQueryStatus::Done,
                    )
                    .await?;
                    tracing::debug!(
                        "[PERF] QUERY_ADDRESS_LIST: updated status to done in {:?}, uid={}, chain_code={}",
                        start_update_progress.elapsed(),
                        req.uid,
                        req.chain_code
                    );

                    // 地址同步完成，发送HintScan事件通知Scanner检查状态
                    // 只有在数据库事实已形成后发送
                    if let Ok(context) = crate::context::get_context() {
                        if let Some(event_tx) = context.get_expand_event_tx().await {
                            // best-effort hint, ignore failure
                            use crate::infrastructure::expand_address::event::ExpandEvent;
                            let _ = event_tx.send(ExpandEvent::HintScan).await;
                            tracing::debug!(
                                "Sent HintScan event for uid={}, chain_code={}",
                                req.uid,
                                req.chain_code
                            );
                        }
                    }

                    tracing::debug!("地址同步完成: uid={}, chain_code={}", req.uid, req.chain_code);
                }
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST: processed pagination in {:?}, uid={}, chain_code={}",
                    start_update_progress.elapsed(),
                    req.uid,
                    req.chain_code
                );
                tracing::debug!(
                    "[PERF] QUERY_ADDRESS_LIST end: total_time={:?}, uid={}, chain_code={}, done={}",
                    start_total.elapsed(),
                    req.uid,
                    req.chain_code,
                    done
                );
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

                    // 创建线程安全的容器来收集需要更新的资产键
                    let asset_keys_to_update = Arc::new(Mutex::new(Vec::new()));
                    let asset_keys_to_update_clone = asset_keys_to_update.clone();

                    stream::iter(chunk_vec.into_iter())
                        .for_each_concurrent(10, move |(address, token, coin)| {
                            // 为每个任务创建 span，保证 tracing context 被传递
                            let span = tracing::debug_span!("asset_process", address = %address, batch = batch_idx + 1);
                            let pool = pool_for_tasks.clone();
                            let chain_code = chain_code_for_tasks.clone();
                            let processed = processed_for_tasks.clone();
                            let asset_keys = asset_keys_to_update_clone.clone();

                            async move {
                                let _enter = span.enter();
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

                                let Ok(account) = ApiAccountRepo::find_one_by_address(&address, pool.clone()).await else {
                                    tracing::error!("find_one_by_address failed for {}", address);
                                    return;
                                };

                                // 如果找到账户，添加到需要更新的资产键列表
                                if let Some(account) = account {
                                    let asset_key = AssetKey::new(
                                        &account.wallet_address,
                                        &address,
                                        &chain_code,
                                        &token.token_address,
                                    );
                                    // 使用互斥锁安全地添加到向量
                                    let mut guard = asset_keys.lock().await;
                                    guard.push(asset_key);
                                }

                                // 增加计数，便于外部核对
                                let prev = processed.fetch_add(1, Ordering::SeqCst);
                                tracing::debug!("TASK_DONE address={} batch={} processed_count={}", address, batch_idx + 1, prev + 1);
                                tracing::debug!("finished asset {}", address);
                            }
                        })
                        .await;

                    // 获取需要更新的资产键列表
                    let asset_keys_guard = asset_keys_to_update.lock().await;
                    let asset_keys_to_update: Vec<AssetKey> = asset_keys_guard.clone();
                    tracing::debug!("asset_keys_to_update: {asset_keys_to_update:?}");
                    // 批量更新资产
                    if !asset_keys_to_update.is_empty() {
                        let asset_calc_actor_manager = crate::context::CONTEXT
                            .get()
                            .unwrap()
                            .get_global_asset_calc_actor_manager()
                            .await?;

                        if let Err(e) =
                            asset_calc_actor_manager.update_assets(&asset_keys_to_update).await
                        {
                            tracing::error!(
                                "batch update_assets failed for batch {}: {:?}",
                                batch_idx + 1,
                                e
                            );
                        } else {
                            tracing::debug!(
                                "Successfully batch updated {} assets for batch {}",
                                asset_keys_to_update.len(),
                                batch_idx + 1
                            );
                        }
                    }

                    // 每批完成后发送带 batch 信息的通知（确保唯一）
                    let total_batches = (total_tasks + BATCH_SIZE - 1) / BATCH_SIZE;
                    // 打印并发送通知
                    tracing::debug!(
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
