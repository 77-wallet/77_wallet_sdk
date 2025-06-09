use std::collections::HashSet;
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{assets::AssetsId, coin::CoinId},
    repositories::{
        assets::AssetsRepoTrait, chain::ChainRepoTrait, coin::CoinRepoTrait,
        exchange_rate::ExchangeRateRepoTrait, ResourcesRepo,
    },
};
use wallet_transport_backend::{
    request::{TokenQueryPrice, TokenQueryPriceReq},
    response_vo::coin::TokenHistoryPrices,
};

use crate::{
    domain::{self, account::AccountDomain, coin::CoinDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, Task, Tasks},
    response_vo::coin::{CoinInfoList, TokenCurrencies, TokenPriceChangeRes},
};

pub struct CoinService {
    pub repo: ResourcesRepo,
    account_domain: AccountDomain,
    coin_domain: CoinDomain,
    // keystore: wallet_crypto::Keystore
}

impl CoinService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            account_domain: AccountDomain::new(),
            coin_domain: CoinDomain::new(),
        }
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
        crate::ServiceError,
    > {
        let tx = &mut self.repo;

        let accounts = self
            .account_domain
            .get_addresses(tx, address, account_id, chain_code.clone(), is_multisig)
            .await?;

        let addresses = accounts
            .into_iter()
            .map(|address| address.address)
            .collect::<Vec<String>>();
        if let Some(is_multisig) = is_multisig
            && is_multisig
        {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
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
        let symbol_list = tx
            .get_chain_assets_by_address_chain_code_symbol(
                addresses,
                chain_code.clone(),
                None,
                _is_multisig,
            )
            .await
            .map_err(crate::ServiceError::Database)?;

        // tracing::info!("[get_hot_coin_list] symbol_list: {symbol_list:#?}");
        let symbol_list: std::collections::HashSet<String> =
            symbol_list.into_iter().map(|coin| coin.symbol).collect();

        // tracing::error!("local_coin_list: {symbol_list:?}");

        let chain_codes = if let Some(chain_code) = chain_code {
            HashSet::from([chain_code])
        } else {
            tx.get_chain_list()
                .await?
                .into_iter()
                .map(|chain| chain.chain_code)
                .collect()
        };

        tracing::debug!("[get_hot_coin_list] hot_coin_list_symbol_not_in start");
        let list = tx
            .hot_coin_list_symbol_not_in(&chain_codes, keyword, &symbol_list, page, page_size)
            .await?;
        let mut data = CoinInfoList::default();
        for coin in list.data {
            if let Some(d) = data
                .iter_mut()
                .find(|d: &&mut crate::response_vo::coin::CoinInfo| d.symbol == coin.symbol)
            {
                d.chain_list.insert(crate::response_vo::coin::ChainInfo {
                    chain_code: coin.chain_code.clone(),
                    token_address: coin.token_address().clone(),
                    protocol: coin.protocol.clone(),
                });
            } else {
                data.push(crate::response_vo::coin::CoinInfo {
                    symbol: coin.symbol.clone(),
                    name: Some(coin.name.clone()),
                    chain_list: HashSet::from([crate::response_vo::coin::ChainInfo {
                        chain_code: coin.chain_code.clone(),
                        token_address: coin.token_address(),
                        protocol: coin.protocol,
                    }]),
                    is_multichain: false,
                })
            }
        }

        data.mark_multi_chain_assets();

        let res = wallet_database::pagination::Pagination {
            page,
            page_size,
            total_count: list.total_count,
            data: data.0,
        };

        Ok(res)
    }

    pub async fn pull_hot_coins(&mut self) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let tx = &mut self.repo;
        tx.drop_coin_just_null_token_address().await?;

        // let list: Vec<wallet_transport_backend::CoinInfo> =
        //     crate::default_data::coins::init_default_coins_list()?
        //         .iter()
        //         .map(|coin| coin.to_owned().into())
        //         .collect();
        // // let exclude_name_list: Vec<String> =
        // //     list.iter().flat_map(|coin| coin.symbol.clone()).collect();
        // self.upsert_hot_coin_list(list, 1, 1).await?;

        let mut data = Vec::new();
        let page_size = 1000;
        let mut page = 0;

        // 拉取远程分页数据，按页获取并追加到 `data` 中
        loop {
            let req = wallet_transport_backend::request::TokenQueryByPageReq::new_default_token(
                Vec::new(), // 空的 exclude_name_list
                page,
                page_size,
            );
            match backend_api.token_query_by_page(cryptor, &req).await {
                Ok(mut list) => {
                    data.append(&mut list.list);
                    page += 1;
                    if page >= list.total_page {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("get_token_price error: {e:?}");
                    break; // 出错时中断循环
                }
            }
        }

        // 拉取流行币种数据并追加到 `data`
        let req =
            wallet_transport_backend::request::TokenQueryByPageReq::new_popular_token(0, page_size);
        // 获取默认的 coins list
        // let default_list: Vec<wallet_transport_backend::CoinInfo> =
        //     crate::default_data::coin::init_default_coins_list()?
        //         .coins
        //         .iter()
        //         .map(|coin| coin.to_owned().into())
        //         .collect();
        if let Ok(mut list) = backend_api.token_query_by_page(cryptor, &req).await {
            data.append(&mut list.list);
        }
        // tracing::info!("pull hot coins data: {data:#?}");
        let filtered_data: Vec<_> = data
            .into_iter()
            .map(|mut d| {
                if d.token_address().is_none() {
                    d.token_address = Some("".to_string());
                };
                d
            })
            .collect();
        // tracing::info!("filtered_data: {filtered_data:?}");
        let data = filtered_data.into_iter().map(|d| d.into()).collect();

        CoinDomain::upsert_hot_coin_list(tx, data).await?;
        // self.upsert_hot_coin_list(data, 0, 1).await?;

        Ok(())
    }

    pub async fn init_token_price(mut self) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let tx = &mut self.repo;

        let coin_list = tx.coin_list(None, None).await?;

        let req: Vec<TokenQueryPrice> = coin_list
            .into_iter()
            .map(|coin| TokenQueryPrice {
                chain_code: coin.chain_code,
                contract_address_list: vec![coin.token_address.unwrap_or_default()],
            })
            .collect();

        let tokens = backend_api
            .token_query_price(
                cryptor,
                wallet_transport_backend::request::TokenQueryPriceReq(req),
            )
            .await?
            .list;
        for token in tokens {
            let coin_id = CoinId {
                chain_code: token.chain_code.clone(),
                symbol: token.symbol.clone(),
                token_address: token.token_address.clone(),
            };
            tx.update_price_unit(&coin_id, &token.price.to_string(), token.unit)
                .await?;
            // tx.update_status(&token.chain_code, &token.symbol, token.token_address, 1)
            //     .await?;
        }

        Ok(())
    }

    pub async fn query_token_price(
        mut self,
        req: TokenQueryPriceReq,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let tx = &mut self.repo;
        // tracing::warn!("[query_token_price] req: {req:?}");

        let tokens = backend_api.token_query_price(cryptor, req).await?.list;

        // tracing::warn!("[query_token_price] tokens: {tokens:?}");
        for token in tokens {
            let coin_id = CoinId {
                chain_code: token.chain_code.clone(),
                symbol: token.symbol.clone(),
                token_address: token.token_address.clone(),
            };
            tx.update_price_unit(&coin_id, &token.price.to_string(), token.unit)
                .await?;
            tx.update_status(&token.chain_code, &token.symbol, token.token_address, 1)
                .await?;
        }
        Ok(())
    }

    // btc sol bnb eth trx
    // cake usdc usdt
    pub async fn get_token_price(
        mut self,
        symbols: Vec<String>,
    ) -> Result<Vec<TokenPriceChangeRes>, crate::ServiceError> {
        let mut coin_domain = self.coin_domain;

        coin_domain.get_token_price(&mut self.repo, symbols).await
    }

    pub async fn query_token_info(
        self,
        chain_code: &str,
        token_address: &str,
    ) -> Result<crate::response_vo::coin::TokenInfo, crate::ServiceError> {
        let mut tx = self.repo;
        let net = wallet_types::chain::network::NetworkKind::Mainnet;
        domain::chain::check_address(token_address, chain_code.try_into()?, net)?;

        let coin =
            CoinRepoTrait::get_coin_by_chain_code_token_address(&mut tx, chain_code, token_address)
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

            let decimals = chain_instance
                .decimals(token_address)
                .await
                .map_err(|e| match e {
                    wallet_chain_interact::Error::UtilsError(wallet_utils::Error::Parse(_))
                    | wallet_chain_interact::Error::RpcError(_) => {
                        crate::ServiceError::Business(crate::BusinessError::Coin(
                            crate::CoinError::InvalidContractAddress(token_address.to_string()),
                        ))
                    }
                    _ => crate::ServiceError::ChainInteract(e),
                })?;
            if decimals == 0 {
                return Err(crate::ServiceError::Business(crate::BusinessError::Coin(
                    crate::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let symbol = chain_instance.token_symbol(token_address).await?;
            if symbol.is_empty() {
                return Err(crate::ServiceError::Business(crate::BusinessError::Coin(
                    crate::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let name = chain_instance.token_name(token_address).await?;
            if name.is_empty() {
                return Err(crate::ServiceError::Business(crate::BusinessError::Coin(
                    crate::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }

            crate::response_vo::coin::TokenInfo {
                symbol: Some(symbol),
                name: Some(name),
                decimals,
            }
        };

        Ok(res)
    }

    pub async fn customize_coin(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        mut token_address: String,
        protocol: Option<String>,
        is_multisig: bool,
    ) -> Result<(), crate::ServiceError> {
        let net = wallet_types::chain::network::NetworkKind::Mainnet;

        let chain: wallet_types::chain::chain::ChainCode = chain_code.try_into()?;

        match chain {
            wallet_types::chain::chain::ChainCode::Ethereum
            | wallet_types::chain::chain::ChainCode::BnbSmartChain => {
                token_address = wallet_utils::address::to_checksum_address(&token_address);
            }
            _ => {}
        }
        domain::chain::check_address(&token_address, chain, net)?;
        let tx = &mut self.repo;
        let Some(_) = tx.detail_with_node(chain_code).await? else {
            return Err(crate::ServiceError::Business(crate::BusinessError::Chain(
                crate::ChainError::NotFound(chain_code.to_string()),
            )));
        };
        let chain_instance =
            domain::chain::adapter::ChainAdapterFactory::get_transaction_adapter(chain_code)
                .await?;

        let coin =
            CoinRepoTrait::get_coin_by_chain_code_token_address(tx, chain_code, &token_address)
                .await?;
        let (decimals, symbol, name) = if let Some(coin) = coin {
            (coin.decimals, coin.symbol, coin.name)
        } else {
            let decimals = chain_instance
                .decimals(&token_address)
                .await
                .map_err(|e| match e {
                    wallet_chain_interact::Error::UtilsError(wallet_utils::Error::Parse(_))
                    | wallet_chain_interact::Error::RpcError(_) => {
                        crate::ServiceError::Business(crate::BusinessError::Coin(
                            crate::CoinError::InvalidContractAddress(token_address.to_string()),
                        ))
                    }
                    _ => crate::ServiceError::ChainInteract(e),
                })?;
            if decimals == 0 {
                return Err(crate::ServiceError::Business(crate::BusinessError::Coin(
                    crate::CoinError::InvalidContractAddress(token_address.to_string()),
                )));
            }
            let symbol = chain_instance.token_symbol(&token_address).await?;
            let name = chain_instance.token_name(&token_address).await?;
            (decimals, symbol, name)
        };

        let cus_coin = wallet_database::entities::coin::CoinData::new(
            Some(name.clone()),
            &symbol,
            chain_code,
            Some(token_address.to_string()),
            None,
            protocol,
            decimals,
            1,
            0,
            1,
        )
        .with_custom(1);
        let coin = vec![cus_coin];
        tx.upsert_multi_coin(coin).await?;

        let mut account_addresses = self
            .account_domain
            .get_addresses(
                tx,
                address,
                account_id,
                Some(chain_code.to_string()),
                Some(is_multisig),
            )
            .await?;

        tracing::debug!(
            "[customize_coin] account_addresses: {:?}",
            account_addresses
        );
        let account_addresses = account_addresses
            .pop()
            .ok_or(crate::ServiceError::Business(
                crate::BusinessError::Account(crate::AccountError::NotFound(address.to_string())),
            ))?;

        tracing::debug!(
            "[customize_coin] account_addresses pop: {:?}",
            account_addresses
        );
        let is_multisig = if is_multisig { 1 } else { 0 };

        // 查询余额
        let balance = chain_instance
            .balance(&account_addresses.address, Some(token_address.to_string()))
            .await?;
        let balance = wallet_utils::unit::format_to_string(balance, decimals)
            .unwrap_or_else(|_| "0".to_string());

        let assets_id = AssetsId::new(&account_addresses.address, chain_code, &symbol);
        let assets = CreateAssetsVo::new(
            assets_id,
            decimals,
            Some(token_address.to_string()),
            None,
            is_multisig,
        )
        .with_name(&name)
        .with_balance(&balance)
        .with_u256(alloy::primitives::U256::default(), decimals)?;

        tx.upsert_assets(assets).await?;
        let req = wallet_transport_backend::request::CustomTokenInitReq {
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
        let task = Task::Common(CommonTask::QueryCoinPrice(req));
        Tasks::new()
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                token_custom_init_task_data,
            )))
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
    ) -> Result<TokenHistoryPrices, crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let prices = backend_api.query_history_price(cryptor, &req).await?;

        Ok(prices)
    }

    pub async fn query_popular_by_page(
        &mut self,
        // chain_code: Option<String>,
        req: wallet_transport_backend::request::TokenQueryPopularByPageReq,
    ) -> Result<wallet_database::pagination::Pagination<TokenPriceChangeRes>, crate::ServiceError>
    {
        let tx = &mut self.repo;
        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let prices = backend_api.query_popular_by_page(cryptor, &req).await?;

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
                let res =
                    TokenCurrencies::calculate_token_price_changes(val, exchange_rate.rate).await?;
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
}
