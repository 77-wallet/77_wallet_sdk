use wallet_database::{
    entities::{
        api_assets::ApiAssetsEntity, api_chain::NodeBindType, api_coin::ApiCoinEntity,
        api_wallet::ApiWalletType, asset_token_key::AssetTokenKey, assets::AssetsId,
    },
    repositories::{
        api_wallet::{
            account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo, coin::ApiCoinRepo,
            wallet::ApiWalletRepo,
        },
        node::NodeRepo,
    },
};
use wallet_transport_backend::request::{
    ChainRpcListReq, TokenQueryPriceReq, api_wallet::address::ApiAddressInitReq,
};
use wallet_types::chain::chain::ChainCode;

use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        app::config::ConfigDomain,
        chain::{ChainDomain, NodeInfo},
        wallet::WalletDomain,
    },
    infrastructure::{
        chain_node::chain_node_ensurer::ChainNodeEnsurer,
        task_queue::{
            CommonTask,
            backend::{BackendApiTask, BackendApiTaskData},
            task::Tasks,
        },
    },
};

pub struct ApiChainDomain {}

impl ApiChainDomain {
    pub(crate) async fn network_kind_by_chain_code(
        chain_code: &str,
    ) -> Result<wallet_types::chain::network::NetworkKind, crate::error::service::ServiceError>
    {
        let node = Self::get_node(chain_code).await?;
        Ok(ChainDomain::network_kind_from_node_network(&node.network))
    }

    pub(crate) async fn init_chains_api_assets(
        _coins: &[ApiCoinEntity],
        _req: &mut TokenQueryPriceReq,
        api_address_init_req: &mut ApiAddressInitReq,
        chain_list: &[String],
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        account_name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
        is_recover: bool,
    ) -> Result<Vec<String>, crate::error::service::ServiceError> {
        tracing::debug!(uid=%uid, wallet_address=%wallet_address, account_id=%account_index_map.account_id, input_index=%account_index_map.input_index, chains_count=chain_list.len(), "ApiChainDomain: starting init_chains_api_assets");
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let mut created_addresses = Vec::new();

        for chain in chain_list.iter() {
            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = ChainDomain::get_node(chain).await else {
                continue;
            };

            if let Some(account) = ApiAccountRepo::find_one_by_wallet_address_account_id_chain_code(
                &pool,
                &wallet_address,
                account_index_map.account_id,
                chain,
            )
            .await?
            {
                if account.is_init == 1 {
                    continue;
                }
            }

            for address_type in address_types {
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &address_type, node.network.as_str().into()).try_into()?;
                let (account_address, address_init_req) = ApiAccountDomain::derive_subkey(
                    uid,
                    seed,
                    wallet_address,
                    account_index_map,
                    &instance,
                    account_name,
                    is_default_name,
                    api_wallet_type,
                    is_recover,
                )
                .await?;

                if let Some(address_init_req) = address_init_req {
                    api_address_init_req.address_list.add_address(address_init_req);
                }

                created_addresses.push(account_address);
            }
        }

        tracing::debug!(uid=%uid, wallet_address=%wallet_address, account_id=%account_index_map.account_id, input_index=%account_index_map.input_index, created_addresses_count=%created_addresses.len(), "ApiChainDomain: completed init_chains_api_assets");
        Ok(created_addresses)
    }

    pub(crate) async fn upsert_multi_api_chain_than_toggle(
        chains: wallet_transport_backend::response_vo::api_wallet::chain::ApiChainListResp,
    ) -> Result<Vec<String>, crate::error::service::ServiceError> {
        // tracing::warn!("upsert_multi_chain_than_toggle, chains: {:#?}", chains);
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        // let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        // // 本地后端节点
        // let local_backend_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(0)).await?;

        // // 本地配置节点
        // let default_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(1)).await?;

        let mut input = Vec::new();
        let mut chain_codes = Vec::new();
        // let mut has_new_chain = false;
        let account_list = ApiAccountRepo::list(&pool).await?;

        let mut new_chains = Vec::new();
        let app_version = ConfigDomain::get_app_version().await?.app_version;
        for chain in chains.0 {
            let Some(master_token_code) = chain.master_token_code else {
                continue;
            };

            let status = match (
                ConfigDomain::compare_versions(&app_version, &chain.app_version_code),
                chain.enable,
            ) {
                (std::cmp::Ordering::Less, _) => 0,
                (_, true) => 1,
                (_, false) => 0,
            };

            if account_list
                .iter()
                .all(|acc_chain| acc_chain.chain_code != chain.chain_code && chain.enable)
            {
                // has_new_chain = true;
                new_chains.push(chain.chain_code.clone());
            }

            // if local_backend_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // } else if default_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // }

            input.push(
                wallet_database::entities::api_chain::ApiChainCreateVo::new(
                    &chain.name,
                    &chain.chain_code,
                    &[],
                    NodeBindType::AutoBackend,
                    &master_token_code,
                )
                .with_status(status),
            );
            if status == 1 {
                chain_codes.push(chain.chain_code);
            }
        }

        ApiChainRepo::upsert_multi_chain(&pool, input).await?;
        Self::toggle_api_chains(&chain_codes).await?;

        if !chain_codes.is_empty() {
            let chain_rpc_list_req = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
                &ChainRpcListReq::new(chain_codes),
            )?;
            Tasks::new().push(BackendApiTask::BackendApi(chain_rpc_list_req)).send().await?;
        }

        Ok(new_chains)
    }

    pub async fn toggle_api_chains(
        chain_codes: &[String],
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        ApiChainRepo::toggle_chains_status(&pool, chain_codes).await?;
        Ok(())
    }

    pub async fn init_api_chain_info() -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let list = crate::default_data::chain::get_default_chains_list()?;

        // tracing::warn!("list {:#?}", list);

        let mut chain_codes = Vec::new();
        for (chain_code, default_chain) in &list.chains {
            let status = if default_chain.active { 1 } else { 0 };
            // let node_id =
            //     NodeDomain::gen_node_id(&default_chain.node_name, &default_chain.chain_code);
            let req = wallet_database::entities::api_chain::ApiChainCreateVo::new(
                &default_chain.name,
                &default_chain.chain_code,
                &default_chain.protocols,
                NodeBindType::AutoLocal,
                &default_chain.main_symbol,
            )
            .with_status(status);

            if let Err(e) = ApiChainRepo::add(&pool, req).await {
                tracing::error!("Failed to create default chain: {:?}", e);
                continue;
            }
            if status == 1 {
                chain_codes.push(chain_code.to_string());
            }
        }
        let app_version = ConfigDomain::get_app_version().await?;

        ApiChainRepo::toggle_chains_status(&pool, &chain_codes).await?;
        let chain_list_req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::api_wallet::API_WALLET_CHAIN_LIST,
            &wallet_transport_backend::request::ChainListReq::new(app_version.app_version),
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(chain_list_req)).send().await?;
        Ok(())
    }

    pub async fn sync_withdrawal_wallet_chain_data()
    -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let password = ApiWalletDomain::get_passwd().await?;

        let chain_list: Vec<String> = ApiChainRepo::get_chain_list(&pool)
            .await?
            .into_iter()
            .map(|chain| chain.chain_code)
            .collect();

        let withdrawal_wallet_list =
            ApiWalletRepo::list(&pool, Some(ApiWalletType::Withdrawal)).await?;

        for wallet in withdrawal_wallet_list {
            ApiAccountDomain::create_withdrawal_account(
                &wallet.address,
                chain_list.clone(),
                "账户",
                true,
                false,
            )
            .await?;
        }

        Ok(())
    }

    pub(crate) async fn get_node(
        chain_code: &str,
    ) -> Result<NodeInfo, crate::error::service::ServiceError> {
        let core_pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let api_pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let ensurer = ChainNodeEnsurer::new(core_pool.clone(), api_pool.clone());
        let node_id = ensurer.ensure_and_get_api_chain_node(chain_code).await?;

        let node = NodeRepo::detail(&core_pool, &node_id).await?.ok_or(
            crate::error::business::BusinessError::ChainNode(
                crate::error::business::chain_node::ChainNodeError::NodeNotFound,
            ),
        )?;
        Ok(NodeInfo::new(
            &node.chain_code,
            &node.node_id,
            &node.name,
            &node.rpc_url,
            &node.ws_url,
            &node.http_url,
            &node.network,
            node.status,
        ))
    }

    pub async fn sync_chains() -> Result<Vec<String>, crate::error::service::ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let app_version = ConfigDomain::get_app_version().await?;
        let chain_list = backend.api_wallet_chain_list(&app_version.app_version).await?;
        ApiChainDomain::upsert_multi_api_chain_than_toggle(chain_list).await
    }

    pub async fn sync_wallet_chain_data(
        wallet_password: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

        crate::domain::wallet::WalletDomain::validate_password(wallet_password).await?;
        let chain_list: Vec<String> = ApiChainRepo::get_chain_list(&pool)
            .await?
            .into_iter()
            .filter(|c| c.node_id.is_some())
            .map(|chain| chain.chain_code)
            .collect();

        let account_wallet_mapping =
            ApiAccountRepo::account_wallet_mapping(&pool, Some(ApiWalletType::Withdrawal)).await?;
        let mut req = TokenQueryPriceReq(Vec::new());
        let coins = ApiCoinRepo::coin_list(&pool).await?;

        // let password = ApiWalletDomain::get_passwd().await?;
        // 获取当前 epoch
        let current_epoch = ConfigDomain::get_keys_reset_epoch().await?;

        let mut api_address_init_req = ApiAddressInitReq::new(current_epoch);

        for wallet in account_wallet_mapping {
            let account_index_map =
                wallet_utils::address::AccountIndexMap::from_account_id(wallet.account_id)?;

            let seed = ApiWalletDomain::decrypt_seed(wallet_password, &wallet.seed).await?;

            ApiChainDomain::init_chains_api_assets(
                &coins,
                &mut req,
                &mut api_address_init_req,
                &chain_list,
                &seed,
                &account_index_map,
                &wallet.uid,
                &wallet.wallet_address,
                &wallet.account_name,
                false,
                wallet.api_wallet_type,
                false,
            )
            .await?;
        }

        // let device_bind_address_task_data =
        //     DeviceDomain::gen_device_bind_address_task_data().await?;
        let mut tasks = Tasks::new();
        if !api_address_init_req.address_list.0.is_empty() {
            let api_address_init_task_data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
                &api_address_init_req,
            )?;
            tasks = tasks.push(BackendApiTask::BackendApi(api_address_init_task_data));
        }

        tasks
            .push(CommonTask::QueryCoinPrice(req))
            // .push(BackendApiTask::BackendApi(expand_address_task_data))
            .send()
            .await?;

        Ok(())
    }
}

pub struct ApiChainTransDomain;

impl ApiChainTransDomain {
    pub async fn assets(
        chain_code: &str,
        from: &str,
        token_address: AssetTokenKey,
    ) -> Result<ApiAssetsEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;

        let assets_id = AssetsId::new(from, chain_code, token_address);
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id).await?.ok_or(
            crate::error::business::BusinessError::Assets(
                crate::error::business::assets::AssetsError::NotFound,
            ),
        )?;

        Ok(assets)
    }

    pub async fn main_coin(
        chain_code: &str,
    ) -> Result<ApiCoinEntity, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().api_wallet_pool()?;
        let coin = ApiCoinRepo::main_coin(chain_code, &pool).await?;
        Ok(coin)
    }
}
