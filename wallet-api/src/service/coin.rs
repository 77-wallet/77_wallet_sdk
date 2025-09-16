use crate::{
    domain::{
        self,
        account::AccountDomain,
        chain::{ChainDomain, adapter::ChainAdapterFactory},
        coin::{CoinDomain, coin_info_to_coin_data},
    },
    infrastructure::{
        parse_utc_with_error,
        task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
    },
    response_vo::{
        chain::ChainList,
        coin::{CoinInfoList, TokenCurrencies, TokenPriceChangeRes},
    },
};
use std::collections::HashMap;
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        assets::AssetsId,
        coin::{BatchCoinSwappable, CoinData, CoinId},
    },
    repositories::{
        ResourcesRepo,
        assets::AssetsRepoTrait,
        coin::{CoinRepo, CoinRepoTrait},
        exchange_rate::ExchangeRateRepoTrait,
    },
};
use wallet_transport_backend::{
    request::TokenQueryPriceReq,
    response_vo::coin::{CoinMarketValue, TokenHistoryPrices},
};

pub struct CoinService {
    pub repo: ResourcesRepo,
    account_domain: AccountDomain,
}

impl CoinService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo, account_domain: AccountDomain::new() }
    }

    pub async fn get_hot_coin_list(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        mut chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
        page: i64,
        page_size: i64,
    ) -> Result<
        wallet_database::pagination::Pagination<crate::response_vo::coin::CoinInfo>,
        crate::error::service::ServiceError,
    > {
        let tx = &mut self.repo;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        let accounts = self
            .account_domain
            .get_addresses(tx, address, account_id, chain_codes, is_multisig)
            .await?;

        let addresses =
            accounts.into_iter().map(|address| address.address).collect::<Vec<String>>();
        if let Some(is_multisig) = is_multisig
            && is_multisig
        {
            let multisig =
                domain::multisig::MultisigDomain::account_by_address(address, true, &pool).await?;
            chain_code = Some(multisig.chain_code);
        }

        // 获取所有链的资产
        let _is_multisig = if let Some(is_multisig) = is_multisig
            && !is_multisig
        {
            None
        } else {
            is_multisig
        };
        let assets = tx
            .get_chain_assets_by_address_chain_code_symbol(
                addresses,
                chain_code.clone(),
                None,
                _is_multisig,
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

        tracing::debug!("[get_hot_coin_list] hot_coin_list_symbol_not_in start");
        let list =
            tx.hot_coin_list_symbol_not_in(&exclude, chain_code, keyword, page, page_size).await?;

        let show_contract = keyword.is_some();
        let mut data = CoinInfoList::default();
        for coin in list.data {
            if let Some(d) = data
                .iter_mut()
                .find(|info| info.symbol == coin.symbol && info.is_default && coin.is_default == 1)
            {
                d.chain_list
                    .entry(coin.chain_code.clone())
                    .or_insert(coin.token_address.unwrap_or_default());
            } else {
                data.push(crate::response_vo::coin::CoinInfo {
                    symbol: coin.symbol.clone(),
                    name: Some(coin.name.clone()),
                    chain_list: ChainList(HashMap::from([(
                        coin.chain_code.clone(),
                        coin.token_address.unwrap_or_default(),
                    )])),
                    is_default: coin.is_default == 1,
                    hot_coin: coin.status == 1,
                    show_contract,
                })
            }
        }

        // let pool = tx.pool();
        // AssetsDomain::show_contract(&pool, keyword, &mut data).await?;

        let res = wallet_database::pagination::Pagination {
            page,
            page_size,
            total_count: list.total_count,
            data: data.0,
        };

        Ok(res)
    }

    pub async fn pull_hot_coins(&mut self) -> Result<(), crate::error::service::ServiceError> {
        // 删除掉无效的token
        let tx = &mut self.repo;

        tx.drop_coin_just_null_token_address().await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 拉所有的币
        let coins = CoinDomain::fetch_all_coin(&pool).await?;

        let data = coins.into_iter().map(|d| coin_info_to_coin_data(d)).collect::<Vec<CoinData>>();
        CoinDomain::upsert_hot_coin_list(tx, data).await?;

        // TODO 1.6版本,修改那些能兑换的代币配置 1.7后面再调整
        let api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let coin = api.swappable_coin().await?;

        let swap_coins = coin
            .into_iter()
            .map(|c| BatchCoinSwappable {
                symbol: c.symbol,
                chain_code: c.chain_code,
                token_address: c.token_address,
            })
            .collect::<Vec<_>>();
        CoinRepo::multi_update_swappable(swap_coins, &pool).await?;
        // TODO 1.6版本,修改那些能兑换的代币配置 1.7后面再调整

        Ok(())
    }

    pub async fn init_token_price(mut self) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let update_at = if let Some(last_coin) = CoinRepo::last_coin(&pool, false).await? {
            last_coin.updated_at.map(|s| s.format("%Y-%m-%d %H:%M:%S").to_string())
        } else {
            None
        };

        let coins = backend_api.fetch_all_tokens(None, update_at).await?;

        let tx = &mut self.repo;
        for token in coins {
            let status = token.get_status();
            let time = parse_utc_with_error(&token.update_time).ok();

            let coin_id = CoinId {
                chain_code: token.chain_code.unwrap_or_default(),
                symbol: token.symbol.unwrap_or_default(),
                token_address: token.token_address.clone(),
            };

            tx.update_price_unit(
                &coin_id,
                &token.price.unwrap_or_default().to_string(),
                token.decimals,
                status,
                Some(token.swappable),
                time,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn query_token_price(
        mut self,
        req: &TokenQueryPriceReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let tx = &mut self.repo;

        let tokens = backend_api.token_query_price(&req).await?.list;

        for token in tokens {
            let coin_id = CoinId {
                chain_code: token.chain_code.clone(),
                symbol: token.symbol.clone(),
                token_address: token.token_address.clone(),
            };
            let status = token.get_status();
            let time = None;
            tx.update_price_unit(
                &coin_id,
                &token.price.to_string(),
                token.unit,
                status,
                token.swappable,
                time,
            )
            .await?;
        }
        Ok(())
    }

    // 查询价格 顺便更新一次币价·
    pub async fn get_token_price(
        mut self,
        symbols: Vec<String>,
    ) -> Result<Vec<TokenPriceChangeRes>, crate::error::service::ServiceError> {
        let tx = &mut self.repo;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let coins = tx.coin_list_with_symbols(&symbols, None).await?;
        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
        coins.into_iter().for_each(|coin| {
            let contract_address = coin.token_address.clone().unwrap_or_default();
            req.insert(&coin.chain_code, &contract_address);
        });

        let tokens = backend_api.token_query_price(&req).await?.list;

        let currency = {
            let config = crate::app_state::APP_STATE.read().await;
            config.currency().to_string()
        };

        let exchange_rate = ExchangeRateRepoTrait::detail(tx, Some(currency.to_string())).await?;

        let mut res = Vec::new();
        if let Some(exchange_rate) = exchange_rate {
            for mut token in tokens {
                if let Some(symbol) =
                    symbols.iter().find(|s| s.to_lowercase() == token.symbol.to_lowercase())
                {
                    token.symbol = symbol.to_string();
                    let coin_id = CoinId {
                        chain_code: token.chain_code.clone(),
                        symbol: symbol.to_string(),
                        token_address: token.token_address.clone(),
                    };
                    let status = if token.enable { Some(1) } else { Some(0) };

                    tx.update_price_unit(
                        &coin_id,
                        &token.price.to_string(),
                        token.unit,
                        status,
                        token.swappable,
                        None,
                    )
                    .await?;
                    let data =
                        TokenCurrencies::calculate_token_price_changes(&token, exchange_rate.rate)
                            .await?;
                    res.push(data);
                }
            }
        }

        Ok(res)
    }

    pub async fn query_token_info(
        self,
        chain_code: &str,
        mut token_address: String,
    ) -> Result<crate::response_vo::coin::TokenInfo, crate::error::service::ServiceError> {
        let mut tx = self.repo;
        let net = wallet_types::chain::network::NetworkKind::Mainnet;
        domain::chain::ChainDomain::check_token_address(&mut token_address, chain_code, net)?;

        let coin = CoinRepoTrait::get_coin_by_chain_code_token_address(
            &mut tx,
            chain_code,
            &token_address,
        )
        .await?;
        let res = if let Some(coin) = coin {
            crate::response_vo::coin::TokenInfo {
                symbol: Some(coin.symbol),
                name: Some(coin.name),
                decimals: coin.decimals,
            }
        } else {
            let chain_instance =
                domain::chain::adapter::ChainAdapterFactory::get_transaction_adapter(chain_code)
                    .await?;

            let decimals = chain_instance.decimals(&token_address).await.map_err(|e| match e {
                wallet_chain_interact::Error::UtilsError(wallet_utils::Error::Parse(_))
                | wallet_chain_interact::Error::RpcError(_) => {
                    crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                        crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                    ))
                }
                _ => crate::error::service::ServiceError::ChainInteract(e),
            })?;
            if decimals == 0 {
                return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                    crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let symbol = chain_instance.token_symbol(&token_address).await?;
            if symbol.is_empty() {
                return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                    crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let name = chain_instance.token_name(&token_address).await?;
            if name.is_empty() {
                return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                    crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }

            crate::response_vo::coin::TokenInfo { symbol: Some(symbol), name: Some(name), decimals }
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
    ) -> Result<(), crate::error::service::ServiceError> {
        let net = wallet_types::chain::network::NetworkKind::Mainnet;

        ChainDomain::check_token_address(&mut token_address, chain_code, net)?;

        let tx = &mut self.repo;
        let _ = ChainDomain::get_node(chain_code).await?;

        let chain_instance = ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        let coin =
            CoinRepoTrait::get_coin_by_chain_code_token_address(tx, chain_code, &token_address)
                .await?;
        let (decimals, symbol, name) = if let Some(coin) = coin {
            (coin.decimals, coin.symbol, coin.name)
        } else {
            let decimals = chain_instance.decimals(&token_address).await.map_err(|e| match e {
                wallet_chain_interact::Error::UtilsError(wallet_utils::Error::Parse(_))
                | wallet_chain_interact::Error::RpcError(_) => {
                    crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                        crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                    ))
                }
                _ => crate::error::service::ServiceError::ChainInteract(e),
            })?;
            if decimals == 0 {
                return Err(crate::error::service::ServiceError::Business(crate::error::business::BusinessError::Coin(
                    crate::error::business::coin::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let symbol = chain_instance.token_symbol(&token_address).await?;
            let name = chain_instance.token_name(&token_address).await?;

            let time = wallet_utils::time::now();
            // TODO 后续优化 用户自定义添加的币种默认不可兑换
            let cus_coin = wallet_database::entities::coin::CoinData::new(
                Some(name.clone()),
                &symbol,
                chain_code,
                Some(token_address.to_string()),
                None,
                protocol,
                decimals,
                0,
                0,
                0,
                false,
                time,
                time,
            )
            .with_custom(1);
            let coin = vec![cus_coin];
            tracing::warn!("[customize_coin] coin: {:?} ", coin);
            tx.upsert_multi_coin(coin).await?;

            (decimals, symbol, name)
        };

        let mut account_addresses = self
            .account_domain
            .get_addresses(tx, address, account_id, vec![chain_code.to_string()], Some(is_multisig))
            .await?;

        tracing::debug!("[customize_coin] account_addresses: {:?}", account_addresses);
        let account_addresses = account_addresses.pop().ok_or(crate::error::service::ServiceError::Business(
            crate::error::business::BusinessError::Account(crate::error::business::account::AccountError::NotFound(address.to_string())),
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
        let assets = CreateAssetsVo::new(assets_id, decimals, None, is_multisig)
            .with_name(&name)
            .with_balance(&balance)
            .with_u256(alloy::primitives::U256::default(), decimals)?;

        tx.upsert_assets(assets).await?;
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

    // .query_history_price(chain_code, symbol, start_time, end_time)

    //     #[derive(Debug, serde::Deserialize, serde::Serialize)]
    // #[serde(rename_all = "camelCase")]
    // pub struct CoinHistoryPrice {
    //     pub data: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    //     pub price: wallet_types::Decimal,
    // }

    // {
    //   "chainCode": "tron",
    //   "code": "trx",
    //   "dateType": "DAY",
    //   "currency": "usd"
    // }
    pub async fn query_history_price(
        self,
        req: wallet_transport_backend::request::TokenQueryHistoryPrice,
    ) -> Result<TokenHistoryPrices, crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let prices = backend_api.query_history_price(&req).await?;

        Ok(prices)
    }

    pub async fn query_popular_by_page(
        &mut self,
        // chain_code: Option<String>,
        req: wallet_transport_backend::request::TokenQueryPopularByPageReq,
    ) -> Result<wallet_database::pagination::Pagination<TokenPriceChangeRes>, crate::error::service::ServiceError>
    {
        let tx = &mut self.repo;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();

        let prices = backend_api.query_popular_by_page(&req).await?;

        let list = prices.list;

        // 筛选list中符合chain_code的
        // let filtered_list = if let Some(code) = chain_code {
        //     list.into_iter()
        //         .filter(|item| item.chain_code == code)
        //         .collect::<Vec<_>>()
        // } else {
        //     list
        // };

        let total_count = list.len() as i64;

        let config = crate::app_state::APP_STATE.read().await;
        let currency = config.currency();

        let exchange_rate = ExchangeRateRepoTrait::detail(tx, Some(currency.to_string())).await?;

        let mut data = Vec::new();
        if let Some(exchange_rate) = exchange_rate {
            for val in list {
                let res = TokenCurrencies::calculate_token_price_changes(&val, exchange_rate.rate)
                    .await?;
                data.push(res);
            }
        }

        let res = wallet_database::pagination::Pagination {
            page: req.page_num,
            page_size: req.page_size,
            total_count,
            data,
        };
        Ok(res)
    }

    pub async fn market_value(
        self,
        coin: std::collections::HashMap<String, String>,
    ) -> Result<CoinMarketValue, crate::error::service::ServiceError> {
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend_api.coin_market_value(coin).await?)
    }
}
