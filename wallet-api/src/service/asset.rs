use crate::{
    domain::{
        account::AccountDomain, assets::AssetsDomain, coin::CoinDomain, multisig::MultisigDomain,
    },
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
    response_vo::assets::{
        AccountChainAsset, AccountChainAssetList, CoinAssets, GetAccountAssetsRes,
    },
};
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        assets::{AssetsEntity, AssetsId},
        coin::SymbolId,
    },
    repositories::{
        ResourcesRepo, account::AccountRepoTrait, assets::AssetsRepoTrait, chain::ChainRepoTrait,
        coin::CoinRepoTrait, device::DeviceRepoTrait,
    },
};
use wallet_transport_backend::request::{
    TokenBalanceRefresh, TokenBalanceRefreshReq, TokenQueryPriceReq,
};

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct AssetsService {
    pub repo: ResourcesRepo,
    account_domain: AccountDomain,
    assets_domain: AssetsDomain,
    coin_domain: CoinDomain, // keystore: wallet_crypto::Keystore
                             // keystore: wallet_crypto::Keystore
}

impl AssetsService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self {
            repo,
            account_domain: AccountDomain::new(),
            assets_domain: AssetsDomain::new(),
            coin_domain: CoinDomain::new(),
        }
    }

    pub async fn get_multisig_account_assets(
        &mut self,
        address: &str,
    ) -> Result<GetAccountAssetsRes, crate::ServiceError> {
        let tx = &mut self.repo;
        let token_currencies = self.coin_domain.get_token_currencies_v2(tx).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let multisig = MultisigDomain::account_by_address(address, true, &pool).await?;
        let address = vec![multisig.address];

        let mut data = tx.get_coin_assets_in_address(address).await?;
        let account_total_assets = token_currencies
            .calculate_account_total_assets(&mut data)
            .await?;

        Ok(GetAccountAssetsRes {
            account_total_assets,
        })
    }

    pub async fn get_account_assets(
        &mut self,
        account_id: u32,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> Result<GetAccountAssetsRes, crate::ServiceError> {
        let tx = &mut self.repo;
        let chains = tx.get_chain_list().await?;
        let chain_codes = if let Some(chain_code) = chain_code {
            vec![chain_code]
        } else {
            chains
                .iter()
                .map(|chain| chain.chain_code.clone())
                .collect()
        };

        let mut data = self
            .assets_domain
            .get_account_assets_entity(tx, account_id, wallet_address, chain_codes, Some(false))
            .await?;
        let token_currencies = self.coin_domain.get_token_currencies_v2(tx).await?;

        let account_total_assets = token_currencies
            .calculate_account_total_assets(&mut data)
            .await?;

        Ok(GetAccountAssetsRes {
            account_total_assets,
        })
    }

    pub async fn detail(
        &mut self,
        address: &str,
        account_id: Option<u32>,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
    ) -> Result<CoinAssets, crate::ServiceError> {
        let tx = &mut self.repo;

        let token_currencies = self.coin_domain.get_token_currencies_v2(tx).await?;
        let address = if let Some(account_id) = account_id {
            let account = tx
                .detail_by_wallet_address_and_account_id_and_chain_code(
                    address, account_id, chain_code,
                )
                .await?
                .ok_or(crate::BusinessError::Account(
                    crate::AccountError::NotFound(address.to_string()),
                ))?;
            account.address
        } else {
            address.to_string()
        };
        let assets_id = AssetsId::new(&address, chain_code, symbol, token_address);
        let assets = tx
            .assets_by_id(&assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        let balance = token_currencies.calculate_assets_entity(&assets).await?;
        let data: CoinAssets = (balance, assets).into();

        Ok(data)
    }

    pub async fn get_all_account_assets(
        &mut self,
        account_id: u32,
        wallet_address: Option<&str>,
    ) -> Result<GetAccountAssetsRes, crate::ServiceError> {
        let tx = &mut self.repo;
        let accounts = tx
            .get_account_list_by_wallet_address_and_account_id(wallet_address, Some(account_id))
            .await?;
        let token_currencies = self.coin_domain.get_token_currencies_v2(tx).await?;

        let addresses = accounts.into_iter().map(|info| info.address).collect();

        let mut data = tx.get_coin_assets_in_address(addresses).await?;

        let account_total_assets = token_currencies
            .calculate_account_total_assets(&mut data)
            .await?;
        Ok(GetAccountAssetsRes {
            account_total_assets,
        })
    }

    // 指定账户下的链的资产列表，需要去重
    // pub async fn get_account_chain_assets(
    //     &self,
    //     address: &str,
    //     account_id: Option<u32>,
    //     chain_code: Option<String>, // mut get_chain: wallet_entity::resources::assets::GetChain,
    //     is_multisig: Option<bool>,
    // ) -> Result<Vec<AccountChainAsset>, crate::ServiceError> {
    //     let service = Service::default();
    //     let account_addresses = self
    //         .account_domain
    //         .get_addresses(address, account_id, chain_code.clone(), is_multisig)
    //         .await?;
    //     let mut account_addresses = account_addresses
    //         .into_iter()
    //         .map(|address| address.address)
    //         .collect::<Vec<String>>();
    //     let mut res = Vec::<AccountChainAsset>::new();

    //     // 根据账户地址、网络查询币资产
    //     for address in account_addresses {
    //         let assets: Vec<AssetsEntity> = service
    //             .asset_service
    //             .get_chain_assets_by_account_address_chain_code_symbol(
    //                 vec![address],
    //                 chain_code.clone(),
    //                 None,
    //             )
    //             .await?;

    //         for asset in assets {
    //             let token_currency =
    //                 super::get_current_coin_unit_price(&asset.symbol, &asset.chain_code).await?;

    //             if let Some(existing_asset) = res.iter_mut().find(|a| a.symbol == asset.symbol) {
    //                 // 如果资产已存在，合并
    //                 let balance = wallet_utils::parse_func::decimal_from_str(&asset.balance)?;
    //                 let balacne_f = wallet_utils::parse_func::f64_from_str(&asset.balance)?;
    //                 let config = crate::config::CONFIG.read().unwrap();
    //                 let currency = config.currency();

    //                 let price = token_currency.get_price(currency);

    //                 let BalanceInfo {
    //                     amount,
    //                     currency,
    //                     unit_price,
    //                     fiat_value,
    //                 } = &mut existing_asset.balance;

    //                 let after_balance = *amount + balacne_f;
    //                 *amount = after_balance;
    //                 let fiat_balance = price * after_balance;
    //                 *fiat_value = Some(fiat_balance);

    //                 // existing_asset.usdt_balance = (after_balance * unit_price).to_string();
    //                 // FIXME: btc 的资产是 非multisig 的，需要特殊处理
    //                 existing_asset.is_multichain = true;
    //             } else {
    //                 let balance = (token_currency, &asset).try_into()?;
    //                 res.push(AccountChainAsset {
    //                     chain_code: asset.chain_code,
    //                     symbol: asset.symbol,
    //                     balance,
    //                     is_multichain: false,
    //                     is_multisig: asset.is_multisig, // chains: vec![chain_assets],
    //                 });
    //             }
    //         }
    //     }

    //     // 过滤掉multisig的资产
    //     if let Some(is_multisig) = is_multisig {
    //         res = res
    //             .into_iter()
    //             .filter(|asset| {
    //                 if is_multisig {
    //                     asset.is_multisig == 1
    //                 } else {
    //                     asset.is_multisig == 0 || asset.is_multisig == 2
    //                 }
    //             })
    //             .collect();
    //     }

    //     Ok(res)
    // }

    // 指定账户下的链的资产列表，需要去重
    pub async fn get_account_chain_assets_v2(
        mut self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<AccountChainAssetList, crate::ServiceError> {
        let mut tx = self.repo;
        let account_addresses = self
            .account_domain
            .get_addresses(
                &mut tx,
                address,
                account_id,
                chain_code.clone(),
                is_multisig,
            )
            .await?;

        // tracing::debug!("account_addresses: {:?}", account_addresses);

        let mut res = AccountChainAssetList::default();
        let token_currencies = self.coin_domain.get_token_currencies_v2(&mut tx).await?;
        // 根据账户地址、网络查询币资产
        for address in account_addresses {
            let assets: Vec<AssetsEntity> = tx
                .get_chain_assets_by_address_chain_code_symbol(
                    vec![address.address],
                    Some(address.chain_code),
                    None,
                    None,
                )
                .await?;
            for asset in assets {
                if let Some(existing_asset) = res.iter_mut().find(|a| a.symbol == asset.symbol) {
                    token_currencies
                        .calculate_assets(asset, existing_asset)
                        .await?;
                } else {
                    let balance = token_currencies.calculate_assets_entity(&asset).await?;

                    // if balance.unit_price == Some(0.0) {
                    //     continue;
                    // }
                    let chain_code = if chain_code.is_none()
                        && let Some(chain) = tx.detail_with_main_symbol(&asset.symbol).await?
                    {
                        chain.chain_code
                    } else {
                        asset.chain_code
                    };

                    res.push(AccountChainAsset {
                        chain_code: chain_code.clone(),
                        symbol: asset.symbol,
                        name: asset.name,
                        // chain_list: HashMap::from([(chain_code, asset.token_address)]),
                        balance,
                        is_multichain: false,
                        is_multisig: asset.is_multisig, // chains: vec![chain_assets],
                    });
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
        res.mark_multichain_assets();
        res.sort_account_chain_assets();
        Ok(res)
    }

    // TODO: 有问题：两个相同chainCode相同symbol的不知道怎么添加
    pub async fn add_coin(
        self,
        address: &str,
        account_id: Option<u32>,
        symbol: &str,
        chain_code: Option<String>,
        // token_address: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<(), crate::ServiceError> {
        let mut tx = self.repo;
        let accounts = self
            .account_domain
            .get_addresses(
                &mut tx,
                address,
                account_id,
                chain_code.clone(),
                is_multisig,
            )
            .await?;

        let coins = tx
            .coin_list_v2(Some(symbol.to_string()), chain_code.clone())
            .await?;

        let Some(device) = tx.get_device_info().await? else {
            return Err(crate::BusinessError::Device(crate::DeviceError::Uninitialized).into());
        };
        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());

        let mut token_balance_refresh_req: TokenBalanceRefreshReq =
            TokenBalanceRefreshReq(Vec::new());
        for account in accounts {
            if let Some(coin) = coins
                .iter()
                .find(|coin| coin.chain_code == account.chain_code)
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
                    AssetsId::new(&account.address, chain_code, symbol, coin.token_address());

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
                tx.upsert_assets(assets).await?;
                token_balance_refresh_req
                    .push(TokenBalanceRefresh::new(address, chain_code, &device.sn));
            }
        }

        let task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::TOKEN_BALANCE_REFRESH,
            &token_balance_refresh_req,
        )?;

        Tasks::new()
            .push(CommonTask::QueryCoinPrice(req))
            .push(BackendApiTask::BackendApi(task_data))
            .send()
            .await?;
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
    ) -> Result<(), crate::ServiceError> {
        let tx = &mut self.repo;
        let accounts = self
            .account_domain
            .get_addresses(tx, address, account_id, None, is_multisig)
            .await?
            .into_iter()
            .map(|account| account.address)
            .collect();
        let assets = tx
            .get_chain_assets_by_address_chain_code_symbol(accounts, None, Some(symbol), None)
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
        tx.delete_multi_assets(assets_ids).await?;

        let mut should_drop_coin = std::collections::HashSet::new();
        for coin in coin_ids {
            let asset = tx
                .get_chain_assets_by_address_chain_code_symbol(
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

        tx.drop_multi_custom_coin(should_drop_coin).await?;

        Ok(())
    }

    pub async fn get_coin_list(
        self,
        address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<crate::response_vo::coin::CoinInfoList, crate::ServiceError> {
        let mut tx = self.repo;
        let account_addresses = self
            .account_domain
            .get_addresses(
                &mut tx,
                address,
                account_id,
                chain_code.clone(),
                is_multisig,
            )
            .await?;
        let account_addresses = account_addresses
            .into_iter()
            .map(|address| address.address)
            .collect::<Vec<String>>();
        let mut res = self
            .assets_domain
            .get_local_coin_list(&mut tx, account_addresses, chain_code, keyword, is_multisig)
            .await?;
        res.mark_multi_chain_assets();
        Ok(res)
    }

    // 根据地址来同步余额(链)
    pub async fn sync_assets_by_addr(
        self,
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::sync_assets_by_addr_chain(addr, chain_code, symbol).await
    }

    // 从后端同步余额(后端)
    pub async fn sync_assets_from_backend(
        self,
        addr: String,
        chain_code: Option<String>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::async_balance_from_backend_addr(addr, chain_code).await
    }

    // 根据钱包地址来同步资产余额
    pub async fn sync_assets_by_wallet_chain(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::sync_assets_by_wallet(wallet_address, account_id, _symbol).await
    }

    pub async fn sync_assets_by_wallet_backend(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        AssetsDomain::async_balance_from_backend_wallet(wallet_address, account_id).await
    }
}
