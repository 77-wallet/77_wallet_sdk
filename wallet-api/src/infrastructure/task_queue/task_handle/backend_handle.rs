use async_trait::async_trait;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use wallet_database::repositories::{device::DeviceRepo, wallet::WalletRepoTrait};
use wallet_transport_backend::{
    api::BackendApi,
    consts::endpoint,
    request::{ChainRpcListReq, FindConfigByKey},
    response_vo::{app::FindConfigByKeyRes, coin::TokenRates},
};

use crate::{
    FrontendNotifyEvent,
    domain::{app::config::ConfigDomain, chain::ChainDomain, node::NodeDomain},
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
        endpoint::api_wallet::ADDRESS_POOL_EXPAND,
    ]
    .iter()
    .cloned()
    .collect()
});

impl BackendTaskHandle {
    pub async fn do_handle(
        endpoint: &str,
        body: serde_json::Value,
        backend: &BackendApi,
        // wallet_type: WalletType,
    ) -> Result<(), crate::ServiceError> {
        let handler = Self::get_handler(endpoint);
        handler.handle(endpoint, body, backend).await?;

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
    ) -> Result<(), crate::ServiceError>;
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
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let Some(device) = DeviceRepo::get_device_info(pool).await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };

        if device.is_init != 1 {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
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
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        match endpoint {
            endpoint::DEVICE_INIT => {
                let res = backend.post_req_str::<Option<()>>(endpoint, &body).await;
                res?;
                use wallet_database::repositories::device::DeviceRepoTrait as _;
                repo.device_init().await?;
            }
            endpoint::KEYS_V2_INIT => {
                // let invite_code = ConfigDomain::get_invite_code().await?;
                // if invite_code.status.is_none() {
                //     return Err(crate::BusinessError::Device(
                //         crate::DeviceError::InviteStatusNotConfirmed,
                //     )
                //     .into());
                // }

                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(
                        crate::BusinessError::Config(crate::ConfigError::KeysNotReset).into()
                    );
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
            endpoint::DEVICE_EDIT_DEVICE_INVITEE_STATUS => {
                let pool = crate::Context::get_global_sqlite_pool()?;
                let Some(device) = DeviceRepo::get_device_info(pool).await? else {
                    return Err(
                        crate::BusinessError::Device(crate::DeviceError::Uninitialized).into()
                    );
                };

                if device.is_init != 1 {
                    return Err(
                        crate::BusinessError::Device(crate::DeviceError::Uninitialized).into()
                    );
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
                repo.language_init().await?;
                let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
                crate::domain::announcement::AnnouncementDomain::pull_announcement(&mut repo)
                    .await?;
            }
            endpoint::ADDRESS_BATCH_INIT => {
                let status = ConfigDomain::get_keys_reset_status().await?;
                if let Some(status) = status
                    && let Some(false) = status.status
                {
                    return Err(
                        crate::BusinessError::Config(crate::ConfigError::KeysNotReset).into()
                    );
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
                                return Err(crate::BusinessError::Wallet(
                                    crate::WalletError::NotInit,
                                )
                                .into());
                            }
                        }
                        None => {
                            return Err(
                                crate::BusinessError::Wallet(crate::WalletError::NotFound).into()
                            );
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

            endpoint::CHAIN_RPC_LIST => {
                let input = backend
                    .post_req_str::<wallet_transport_backend::response_vo::chain::ChainInfos>(
                        endpoint, &body,
                    )
                    .await?;
                let req = wallet_utils::serde_func::serde_from_value::<ChainRpcListReq>(body)?;
                let mut backend_nodes = Vec::new();
                NodeDomain::upsert_chain_rpc(&mut repo, input, &mut backend_nodes).await?;
                NodeDomain::sync_nodes_and_link_to_chains(
                    &mut repo,
                    req.chain_code,
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
            _ => {
                // 未知的 endpoint
                tracing::warn!("unknown endpoint: {}", endpoint);
                Err(crate::ServiceError::System(crate::SystemError::BackendEndpointNotFound))?;
            }
        }

        Ok(())
    }
}
