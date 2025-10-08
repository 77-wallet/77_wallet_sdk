use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, assets::ApiAssetsDomain},
        app::config::ConfigDomain,
        assets::AssetsDomain,
        chain::adapter::ChainAdapterFactory,
        coin::CoinDomain,
    },
    response_vo::{
        account::{Balance, BalanceInfo},
        api_wallet::assets::{ApiAccountChainAsset, ApiAccountChainAssetList},
        assets::GetAccountAssetsRes,
        chain::ChainList,
        coin::TokenCurrencyId,
    },
};
use std::collections::HashMap;
use wallet_database::{
    entities::{api_assets::ApiCreateAssetsVo, assets::AssetsId},
    repositories::{
        api_wallet::{account::ApiAccountRepo, assets::ApiAssetsRepo, chain::ApiChainRepo},
        coin::CoinRepo,
    },
};
use wallet_utils::unit;

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct ApiAssetsService;

impl ApiAssetsService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn add_assets(
        req: crate::request::coin::AddCoinReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 钱包下的账号
        let accounts = ApiAccountRepo::list_by_wallet_address(
            &pool,
            &req.wallet_address,
            Some(req.account_id),
            None,
        )
        .await?;

        let coins = CoinRepo::coin_list_by_chain_token_map_batch(&pool, &req.chain_list).await?;
        for coin in coins {
            if let Some(account) =
                accounts.iter().find(|account| account.chain_code == coin.chain_code)
            {
                let chain_code = account.chain_code.as_str();

                let assets_id =
                    AssetsId::new(&account.address, chain_code, &coin.symbol, coin.token_address());

                let assets =
                    ApiCreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                        .with_name(&coin.name);

                ApiAssetsRepo::upsert_assets(&pool, assets).await?
            };
        }

        Ok(())
    }

    pub async fn remove_assets(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_list: ChainList,
        _is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;

        for (chain_code, token_address) in chain_list.iter() {
            // 找到对应链的地址
            let account = accounts.iter().find(|account| account.chain_code == *chain_code);

            if let Some(account) = account {
                ApiAssetsRepo::delete_assets(&pool, &account.address, chain_code, token_address)
                    .await?;
            };
        }

        Ok(())
    }

    // 根据后端同步余额
    pub async fn sync_assets_by_wallet_backend(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::async_balance_from_backend_wallet(wallet_address, account_id).await
    }

    pub async fn chain_balance(
        &self,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<Balance, crate::error::service::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let coin = CoinRepo::coin_by_chain_address(chain_code, token_address, &pool).await?;

        let token_address = (!token_address.is_empty()).then_some(token_address.to_string());

        let balance = adapter.balance(address, token_address).await?;
        let format_balance = unit::format_to_string(balance, coin.decimals)?;

        let balance = Balance {
            balance: format_balance.clone(),
            decimals: coin.decimals,
            original_balance: balance.to_string(),
        };

        // 更新本地余额
        ApiAssetsDomain::update_balance(address, chain_code, coin.token_address, &format_balance)
            .await?;

        Ok(balance)
    }

    pub async fn get_api_assets_list(
        self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<ApiAccountChainAssetList, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        let account_addresses =
            ApiAccountDomain::get_addresses(wallet_address, account_id, chain_codes).await?;

        let mut res = ApiAccountChainAssetList::default();
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        // 根据账户地址、网络查询币资产
        for address in account_addresses {
            let assets_list = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
                &pool,
                vec![address.address],
                Some(address.chain_code),
                None,
                None,
            )
            .await?;
            for assets in assets_list {
                let coin = CoinDomain::get_coin(
                    &assets.chain_code,
                    &assets.symbol,
                    assets.token_address(),
                )
                .await?;
                if let Some(existing_asset) = res
                    .iter_mut()
                    .find(|a| a.symbol == assets.symbol && a.is_default && coin.is_default == 1)
                {
                    token_currencies.calculate_api_assets(assets, existing_asset).await?;
                    existing_asset
                        .chain_list
                        .entry(coin.chain_code.clone())
                        .or_insert(coin.token_address.unwrap_or_default());
                } else {
                    let balance = token_currencies.calculate_api_assets_entity(&assets).await?;

                    let chain_code = if chain_code.is_none()
                        && let Some(chain) =
                            ApiChainRepo::detail_with_main_symbol(&pool, &assets.symbol).await?
                    {
                        chain.chain_code
                    } else {
                        assets.chain_code
                    };

                    res.push(ApiAccountChainAsset {
                        chain_code: chain_code.clone(),
                        symbol: assets.symbol,
                        name: assets.name,
                        chain_list: ChainList(HashMap::from([(chain_code, assets.token_address)])),
                        balance,
                        is_multisig: assets.is_multisig, // chains: vec![chain_assets],
                        is_default: coin.is_default == 1,
                    })
                }
            }
        }

        // 过滤掉multisig的资产
        if let Some(is_multisig) = is_multisig {
            res.retain(|asset| {
                if is_multisig {
                    asset.is_multisig == 1
                } else {
                    asset.is_multisig == 0 || asset.is_multisig == 2
                }
            });
        }
        // res.mark_multichain_assets();
        res.sort_account_chain_assets();
        Ok(res)
    }

    // 已添加的资产
    pub async fn get_added_coin_list(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        _is_multisig: Option<bool>,
    ) -> Result<crate::response_vo::coin::CoinInfoList, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        let account_addresses =
            ApiAccountDomain::get_addresses(wallet_address, account_id, chain_codes).await?;

        let address = account_addresses.into_iter().map(|a| a.address).collect::<Vec<_>>();

        let assets = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
            &pool, address, None, None, None,
        )
        .await?;

        let show_contract = keyword.is_some();
        let mut res = crate::response_vo::coin::CoinInfoList::default();
        for assets in assets {
            let coin =
                CoinDomain::get_coin(&assets.chain_code, &assets.symbol, assets.token_address())
                    .await?;
            if let Some(info) =
                res.iter_mut().find(|info| info.symbol == assets.symbol && coin.is_default == 1)
            {
                info.chain_list.entry(assets.chain_code.clone()).or_insert(assets.token_address);
            } else {
                res.push(crate::response_vo::coin::CoinInfo {
                    symbol: assets.symbol,
                    name: Some(assets.name),

                    chain_list: ChainList(HashMap::from([(
                        assets.chain_code.clone(),
                        assets.token_address,
                    )])),
                    is_default: coin.is_default == 1,
                    hot_coin: coin.status == 1,
                    show_contract,
                });
            }
        }

        Ok(res)
    }

    // 单个索引下的所有资产总和
    pub async fn get_account_assets(
        account_id: u32,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> Result<GetAccountAssetsRes, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let account = ApiAccountRepo::list_by_wallet_address(
            &pool,
            wallet_address,
            Some(account_id),
            chain_code.as_deref(),
        )
        .await?;
        let address = account.iter().map(|a| a.address.clone()).collect::<Vec<_>>();

        let mut assets = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
            &pool, address, chain_code, None, None,
        )
        .await?;

        // 币符号
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        let mut account_total_assets = Some(wallet_types::Decimal::default());
        let mut amount = wallet_types::Decimal::default();

        let currency = ConfigDomain::get_currency().await?;

        for assets in assets.iter_mut() {
            let token_currency_id =
                TokenCurrencyId::new(&assets.symbol, &assets.chain_code, assets.token_address());

            let value = if let Some(token_currency) = token_currencies.get(&token_currency_id) {
                let balance = wallet_utils::parse_func::decimal_from_str(&assets.balance)?;
                let price = token_currency.get_price(&currency);
                if let Some(price) = price {
                    let price = wallet_types::Decimal::from_f64_retain(price).unwrap_or_default();
                    Some(price * balance)
                } else {
                    None
                }
            } else {
                None
            };

            amount += wallet_utils::parse_func::decimal_from_str(&assets.balance)?;
            account_total_assets =
                account_total_assets.map(|total| total + value.unwrap_or_default());
        }

        let bal = BalanceInfo {
            amount: wallet_utils::conversion::decimal_to_f64(&amount)?,
            currency: currency.to_string(),
            unit_price: Default::default(),
            fiat_value: account_total_assets
                .map(|total| wallet_utils::conversion::decimal_to_f64(&total))
                .transpose()?,
        };

        Ok(GetAccountAssetsRes { account_total_assets: bal })
    }

    // 资产列表
    pub async fn get_account_chain_assets(
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        _is_multisig: Option<bool>,
    ) -> Result<ApiAccountChainAssetList, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let accounts = ApiAccountRepo::list_by_wallet_address(
            &pool,
            wallet_address,
            account_id,
            chain_code.as_deref(),
        )
        .await?;

        let mut res = ApiAccountChainAssetList::default();
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        // 根据账户地址、网络查询币资产
        for address in accounts {
            let assets_list = ApiAssetsRepo::get_chain_assets_by_address_chain_code_symbol(
                &pool,
                vec![address.address],
                Some(address.chain_code),
                None,
                None,
            )
            .await?;
            for assets in assets_list {
                let coin = CoinDomain::get_coin(
                    &assets.chain_code,
                    &assets.symbol,
                    assets.token_address(),
                )
                .await?;
                if let Some(existing_asset) = res
                    .iter_mut()
                    .find(|a| a.symbol == assets.symbol && a.is_default && coin.is_default == 1)
                {
                    token_currencies.calculate_api_assets(assets, existing_asset).await?;
                    existing_asset
                        .chain_list
                        .entry(coin.chain_code.clone())
                        .or_insert(coin.token_address.unwrap_or_default());
                } else {
                    let balance = token_currencies.calculate_api_assets_entity(&assets).await?;

                    res.push(ApiAccountChainAsset {
                        chain_code: assets.chain_code.clone(),
                        symbol: assets.symbol,
                        name: assets.name,
                        chain_list: ChainList(HashMap::from([(
                            assets.chain_code,
                            assets.token_address,
                        )])),
                        balance,
                        is_multisig: assets.is_multisig,
                        is_default: coin.is_default == 1,
                    })
                }
            }
        }

        res.sort_account_chain_assets();
        Ok(res)
    }
}
