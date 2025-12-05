use crate::{
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, coin::ApiCoinDomain},
        chain::{ChainDomain, adapter::ChainAdapterFactory},
    },
    infrastructure::task_queue::{
        CommonTask,
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
};
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo, api_coin::ApiCoinData, assets::AssetsId, coin::CoinId,
    },
    repositories::api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo, coin::ApiCoinRepo},
};
use wallet_transport_backend::request::TokenQueryPriceReq;
use crate::response_vo::standard_wallet::coin::CoinInfo;

pub struct ApiCoinService {
    ctx: &'static Context,
}

impl ApiCoinService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    // 热门币种列表 排除某个钱包已经添加的币种
    pub async fn get_hot_coin_list(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
        page: i64,
        page_size: i64,
    ) -> Result<
        wallet_database::pagination::Pagination<CoinInfo>,
        crate::error::service::ServiceError,
    > {
        let pool = self.ctx.get_global_sqlite_pool()?;

        // 地址里列表
        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;
        let addresses =
            accounts.into_iter().map(|address| address.address).collect::<Vec<String>>();

        // 获取资产
        let assets = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
            &pool,
            addresses,
            chain_code.clone(),
            None,
            is_multisig,
        )
        .await?;

        let exclude = assets
            .iter()
            .map(|asset| CoinId {
                symbol: asset.symbol.clone(),
                chain_code: asset.chain_code.clone(),
                token_address: asset.token_address(),
            })
            .collect::<Vec<CoinId>>();

        let list = ApiCoinRepo::coin_list_symbol_not_in(
            &pool, &exclude, chain_code, keyword, page, page_size,
        )
        .await?;

        let data = ApiCoinDomain::merge_coin_to_list(list.data, keyword.is_some())?;
        let res = wallet_database::pagination::Pagination {
            page,
            page_size,
            total_count: list.total_count,
            data: data.0,
        };
        Ok(res)
    }

    // 用户自定义添加币种
    pub async fn customize_coin(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        mut token_address: String,
        protocol: Option<String>,
        is_multisig: bool,
        status: u8,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        let net = wallet_types::chain::network::NetworkKind::Mainnet;

        ChainDomain::check_token_address(&mut token_address, chain_code, net)?;

        let _ = ChainDomain::get_node(chain_code).await?;

        let chain_instance = ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        let coin =
            ApiCoinRepo::get_coin_by_chain_code_token_address(&pool, chain_code, &token_address)
                .await?;
        let (decimals, symbol, name) = if let Some(coin) = coin {
            (coin.decimals, coin.symbol, coin.name)
        } else {
            let decimals = chain_instance.decimals(&token_address).await.map_err(|e| match e {
                wallet_chain_interact::Error::UtilsError(wallet_utils::Error::Parse(_))
                | wallet_chain_interact::Error::RpcError(_) => {
                    crate::error::service::ServiceError::Business(
                        crate::error::business::BusinessError::Coin(
                            crate::error::business::coin::CoinError::InvalidContractAddress(
                                token_address.to_string(),
                            ),
                        ),
                    )
                }
                _ => crate::error::service::ServiceError::ChainInteract(e),
            })?;
            if decimals == 0 {
                return Err(crate::error::service::ServiceError::Business(
                    crate::error::business::BusinessError::Coin(
                        crate::error::business::coin::CoinError::InvalidContractAddress(
                            token_address.to_string(),
                        ),
                    ),
                ));
            }
            let symbol = chain_instance.token_symbol(&token_address).await?;
            let name = chain_instance.token_name(&token_address).await?;

            let time = wallet_utils::time::now();
            // TODO 后续优化 用户自定义添加的币种默认不可兑换
            let cus_coin = ApiCoinData::new(
                Some(name.clone()),
                &symbol,
                chain_code,
                Some(token_address.to_string()),
                None,
                protocol,
                decimals,
                0,
                0,
                status,
                time,
                Some(time),
            )
            .with_custom(1);
            let coin = vec![cus_coin];
            tracing::warn!("[customize_coin] coin: {:?} ", coin);
            ApiCoinRepo::upsert_multi_coin(&pool, coin).await?;
            // tx.upsert_multi_coin(coin).await?;

            (decimals, symbol, name)
        };

        let mut account_addresses =
            ApiAccountDomain::get_addresses(address, account_id, vec![chain_code.to_string()])
                .await?;

        tracing::debug!("[customize_coin] account_addresses: {:?}", account_addresses);
        let account_addresses =
            account_addresses.pop().ok_or(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::Account(
                    crate::error::business::account::AccountError::NotFound(address.to_string()),
                ),
            ))?;

        tracing::debug!("[customize_coin] account_addresses pop: {:?}", account_addresses);
        let is_multisig = if is_multisig { 1 } else { 0 };

        // 查询余额
        let balance = chain_instance
            .balance(&account_addresses.address, Some(token_address.to_string()))
            .await?;
        let balance = wallet_utils::unit::format_to_string(balance, decimals)
            .unwrap_or_else(|_| "0".to_string());

        let assets_id = AssetsId::new(
            &account_addresses.address,
            chain_code,
            &symbol,
            Some(token_address.clone()),
        );
        let assets = ApiCreateAssetsVo::new(assets_id, decimals, None, is_multisig)
            .with_name(&name)
            .with_balance(&balance)
            .with_u256(alloy::primitives::U256::default(), decimals)?;

        ApiAssetsRepo::upsert_assets(&pool, assets).await?;
        let req = wallet_transport_backend::request::CustomTokenInitReq {
            address: account_addresses.address,
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_name: name,
            contract_address: Some(token_address.to_string()),
            master: false,
            unit: decimals,
        };
        let token_custom_init_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::TOKEN_CUSTOM_TOKEN_INIT,
            &req,
        )?;

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        req.insert(chain_code, &token_address);
        let task = CommonTask::QueryCoinPrice(req);
        Tasks::new()
            .push(BackendApiTask::BackendApi(token_custom_init_task_data))
            .push(task)
            .send()
            .await?;
        Ok(())
    }
}
