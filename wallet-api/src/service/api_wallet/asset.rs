use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, assets::ApiAssetsDomain},
        assets::AssetsDomain,
        chain::adapter::ChainAdapterFactory,
        coin::CoinDomain,
    },
    response_vo::{
        account::Balance,
        api_wallet::assets::{ApiAccountChainAsset, ApiAccountChainAssetList},
        chain::ChainList,
    },
};
use std::collections::HashMap;
use wallet_database::{
    entities::{api_assets::ApiCreateAssetsVo, assets::AssetsId},
    repositories::{
        api_account::ApiAccountRepo, api_assets::ApiAssetsRepo, api_chain::ApiChainRepo,
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
        self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_list: ChainList,
        _is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 钱包下的账号
        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;

        let coins = CoinRepo::coin_list_by_chain_token_map_batch(&pool, &chain_list).await?;
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
        &mut self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_list: ChainList,
        _is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let accounts =
            ApiAccountRepo::list_by_wallet_address(&pool, wallet_address, account_id, None).await?;

        let coins = CoinRepo::coin_list_by_chain_token_map_batch(&pool, &chain_list).await?;

        // let mut assets_ids = Vec::new();
        // let mut coin_ids = std::collections::HashSet::new();

        // 地址，链，token地址

        // ApiAssetsRepo::
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
}
