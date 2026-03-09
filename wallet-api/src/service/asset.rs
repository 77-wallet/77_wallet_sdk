use crate::{
    domain::{
        account::AccountDomain, assets::AssetsDomain, coin::CoinDomain, multisig::MultisigDomain,
    },
    infrastructure::task_queue::{CommonTask, task::Tasks},
    response_vo::standard_wallet::{
        assets::{AccountChainAsset, AccountChainAssetList, CoinAssets, GetAccountAssetsRes},
        chain::ChainList,
    },
};
use std::collections::HashMap;
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        assets::AssetsId,
        coin::SymbolId,
    },
    repositories::{account::AccountRepo, assets::AssetsRepo, chain::ChainRepo, coin::CoinRepo},
};
use wallet_transport_backend::request::TokenQueryPriceReq;

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct AssetsService {
    account_domain: AccountDomain,
    assets_domain: AssetsDomain,
}

impl AssetsService {
    pub fn new() -> Self {
        Self { account_domain: AccountDomain::new(), assets_domain: AssetsDomain::new() }
    }

    pub async fn get_multisig_account_assets(
        &mut self,
        address: &str,
    ) -> Result<GetAccountAssetsRes, crate::error::service::ServiceError> {
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let multisig = MultisigDomain::account_by_address(address, true, &pool).await?;
        let address = vec![multisig.address];

        let core_pool = wallet_database::CoreDbPool::new(pool.clone());
        let mut data = AssetsRepo::get_coin_assets_in_address(&core_pool, address, Some(1)).await?;
        let account_total_assets =
            token_currencies.calculate_account_total_assets(&mut data).await?;

        Ok(GetAccountAssetsRes { account_total_assets })
    }

    pub async fn get_account_assets(
        &mut self,
        account_id: u32,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> Result<GetAccountAssetsRes, crate::error::service::ServiceError> {
        let pool = crate::context::get_context()?.core_pool()?;
        let chains = ChainRepo::get_chain_list(&pool).await?;
        let chain_codes = if let Some(chain_code) = chain_code {
            vec![chain_code]
        } else {
            chains.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        let mut data = self
            .assets_domain
            .get_account_assets_entity(&pool, account_id, wallet_address, chain_codes, Some(false))
            .await?;
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        let account_total_assets =
            token_currencies.calculate_account_total_assets(&mut data).await?;

        Ok(GetAccountAssetsRes { account_total_assets })
    }

    pub async fn detail(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
    ) -> Result<CoinAssets, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;
        let address = if let Some(account_id) = account_id {
            let account = AccountRepo::detail_by_wallet_address_and_account_id_and_chain_code(
                pool.clone(),
                address,
                account_id,
                chain_code,
            )
            .await?
            .ok_or(crate::error::business::BusinessError::Account(
                crate::error::business::account::AccountError::NotFound(address.to_string()),
            ))?;
            account.address
        } else {
            address.to_string()
        };
        let assets_id = AssetsId::new(&address, chain_code, symbol, token_address);
        let assets = AssetsRepo::assets_by_id(&pool, &assets_id).await?.ok_or(
            crate::error::business::BusinessError::Assets(
                crate::error::business::assets::AssetsError::NotFound,
            ),
        )?;

        let balance = token_currencies.calculate_assets_entity(&assets).await?;
        let data: CoinAssets = (balance, assets).into();

        Ok(data)
    }

    pub async fn get_all_account_assets(
        &mut self,
        account_id: u32,
        wallet_address: Option<&str>,
    ) -> Result<GetAccountAssetsRes, crate::error::service::ServiceError> {
        let core_pool = crate::get_context()?.core_pool()?;
        let accounts = AccountRepo::get_account_list_by_wallet_address_and_account_id(
            core_pool.clone(),
            wallet_address,
            Some(account_id),
        )
        .await?;
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        let addresses = accounts.into_iter().map(|info| info.address).collect();

        let mut data = AssetsRepo::get_coin_assets_in_address(&core_pool, addresses, Some(1)).await?;

        let account_total_assets =
            token_currencies.calculate_account_total_assets(&mut data).await?;
        Ok(GetAccountAssetsRes { account_total_assets })
    }

    // 指定账户下的链的资产列表，需要去重
    pub async fn get_account_chain_assets_v2(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<AccountChainAssetList, crate::error::service::ServiceError> {
        let core_pool = crate::context::get_context()?.core_pool()?;

        let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        let account_addresses = self
            .account_domain
            .get_addresses(address, account_id, chain_codes, is_multisig)
            .await?;

        let mut res = AccountChainAssetList::default();
        let token_currencies = CoinDomain::get_token_currencies_v2().await?;

        // 根据账户地址、网络查询币资产
        for address in account_addresses {
            let assets_list = AssetsRepo::get_chain_assets_by_address_chain_code_symbol(
                &core_pool,
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
                    token_currencies.calculate_assets(assets, existing_asset).await?;
                    existing_asset
                        .chain_list
                        .entry(coin.chain_code.clone())
                        .or_insert(coin.token_address.unwrap_or_default());
                } else {
                    let balance = token_currencies.calculate_assets_entity(&assets).await?;

                    let chain_code = if chain_code.is_none()
                        && let Some(chain) =
                            ChainRepo::detail_with_main_symbol(&core_pool, &assets.symbol).await?
                    {
                        chain.chain_code
                    } else {
                        assets.chain_code
                    };

                    res.push(AccountChainAsset {
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

    pub async fn add_coin_v2(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_list: ChainList,
        is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let core_pool = wallet_database::CoreDbPool::new(pool.clone());
        let chains = chain_list.keys().cloned().collect();
        let accounts =
            self.account_domain.get_addresses(address, account_id, chains, is_multisig).await?;
        let coins = CoinRepo::coin_list_by_chain_token_map_batch(&core_pool, &chain_list).await?;

        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());

        for coin in coins {
            if let Some(account) =
                accounts.iter().find(|account| account.chain_code == coin.chain_code)
            {
                let chain_code = account.chain_code.as_str();
                // let code: ChainCode = chain_code.try_into()?;

                let is_multisig = if let Some(is_multisig) = is_multisig
                    && is_multisig
                {
                    1
                } else {
                    0
                };

                let assets_id =
                    AssetsId::new(&account.address, chain_code, &coin.symbol, coin.token_address());
                let assets = CreateAssetsVo::new(
                    assets_id,
                    coin.decimals,
                    coin.protocol.clone(),
                    is_multisig,
                )
                .with_name(&coin.name)
                .with_u256(alloy::primitives::U256::default(), coin.decimals)?;

                if coin.price.is_empty() {
                    req.insert(
                        chain_code,
                        &assets.assets_id.token_address.clone().unwrap_or_default(),
                    );
                }
                AssetsRepo::upsert_assets(&core_pool, assets).await?;
            }
        }

        Tasks::new().push(CommonTask::QueryCoinPrice(req)).send().await?;
        Ok(())
    }

    pub async fn remove_coin_v2(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        chain_list: ChainList,
        is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let core_pool = wallet_database::CoreDbPool::new(pool.clone());

        let chains = chain_list.keys().cloned().collect();

        let accounts =
            self.account_domain.get_addresses(address, account_id, chains, is_multisig).await?;

        let assets = AssetsRepo::list_by_chain_token_map_batch(&core_pool, &chain_list)
            .await?
            .into_iter()
            .filter(|asset| {
                accounts
                    .iter()
                    .any(|account| account.address == asset.address && account.chain_code == asset.chain_code)
            })
            .collect::<Vec<_>>();
        let mut assets_ids = Vec::new();
        let mut coin_ids = std::collections::HashSet::new();

        for asset in assets {
            let assets_id = AssetsId::new(
                &asset.address,
                &asset.chain_code,
                &asset.symbol,
                Some(asset.token_address),
            );
            assets_ids.push(assets_id);
            let coin_id = SymbolId::new(&asset.chain_code, &asset.symbol);
            coin_ids.insert(coin_id);
        }
        AssetsRepo::delete_multi_assets(&core_pool, assets_ids).await?;

        let mut should_drop_coin = std::collections::HashSet::new();
        for coin in coin_ids {
            let asset = AssetsRepo::get_chain_assets_by_address_chain_code_symbol(
                &core_pool,
                Vec::new(),
                Some(coin.chain_code.clone()),
                Some(&coin.symbol),
                None,
            )
            .await?;
            if asset.is_empty() {
                should_drop_coin.insert(coin);
            }
        }

        CoinRepo::drop_multi_custom_coin(&core_pool, should_drop_coin).await?;

        Ok(())
    }

    // XXX: 移除资产现在是符号相同的都移除，包括自定义的
    pub async fn remove_coin(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        symbol: &str,
        // token_address: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let core_pool = crate::context::get_context()?.core_pool()?;
        let accounts = self
            .account_domain
            .get_addresses(address, account_id, vec![], is_multisig)
            .await?
            .into_iter()
            .map(|account| account.address)
            .collect();
        let assets = AssetsRepo::get_chain_assets_by_address_chain_code_symbol(
            &core_pool,
            accounts,
            None,
            Some(symbol),
            None,
        )
        .await?;
        let mut assets_ids = Vec::new();
        let mut coin_ids = std::collections::HashSet::new();
        for asset in assets {
            let assets_id = AssetsId::new(
                &asset.address,
                &asset.chain_code,
                &asset.symbol,
                Some(asset.token_address),
            );
            assets_ids.push(assets_id);
            let coin_id = SymbolId::new(&asset.chain_code, symbol);
            coin_ids.insert(coin_id);
        }
        AssetsRepo::delete_multi_assets(&core_pool, assets_ids).await?;

        let mut should_drop_coin = std::collections::HashSet::new();
        for coin in coin_ids {
            let asset = AssetsRepo::get_chain_assets_by_address_chain_code_symbol(
                &core_pool,
                Vec::new(),
                Some(coin.chain_code.clone()),
                Some(&coin.symbol),
                None,
            )
            .await?;
            if asset.is_empty() {
                should_drop_coin.insert(coin);
            }
        }

        CoinRepo::drop_multi_custom_coin(&core_pool, should_drop_coin).await?;

        Ok(())
    }

    pub async fn get_coin_list(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<
        crate::response_vo::standard_wallet::coin::CoinInfoList,
        crate::error::service::ServiceError,
    > {
        let core_pool = crate::context::get_context()?.core_pool()?;
        let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        let account_addresses = self
            .account_domain
            .get_addresses(address, account_id, chain_codes, is_multisig)
            .await?;
        let account_addresses =
            account_addresses.into_iter().map(|address| address.address).collect::<Vec<String>>();

        let res = self
            .assets_domain
            .get_local_coin_list(&core_pool, account_addresses, chain_code, keyword, is_multisig)
            .await?;

        Ok(res)
    }

    // 根据地址来同步余额(链)
    pub async fn sync_assets_by_addr(
        self,
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::sync_assets_by_addr_chain(addr, chain_code, symbol).await
    }

    // 从后端同步余额(后端)
    pub async fn sync_assets_from_backend(
        self,
        addr: String,
        chain_code: Option<String>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::async_balance_from_backend_addr(addr, chain_code).await
    }

    // 根据钱包地址来同步资产余额
    pub async fn sync_assets_by_wallet_chain(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::sync_assets_by_wallet(wallet_address, account_id, _symbol).await
    }

    pub async fn sync_assets_by_wallet_backend(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::async_balance_from_backend_wallet(wallet_address, account_id).await
    }
}
