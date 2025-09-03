use super::chain::adapter::ChainAdapterFactory;
use crate::{
    domain::coin::CoinDomain,
    request::transaction::SwapTokenInfo,
    response_vo::{chain::ChainList, coin::CoinInfoList},
};
use futures::{stream, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Semaphore;
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        account::AccountEntity,
        assets::{AssetsEntity, AssetsId},
        coin::{CoinEntity, CoinMultisigStatus},
        wallet::WalletEntity,
    },
    repositories::{
        account::AccountRepoTrait, assets::AssetsRepoTrait, coin::CoinRepo, ResourcesRepo,
    },
    DbPool,
};
use wallet_transport_backend::request::TokenQueryPriceReq;

pub struct AssetsDomain;

impl Default for AssetsDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetsDomain {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_account_assets_entity(
        &mut self,
        repo: &mut ResourcesRepo,
        account_id: u32,
        wallet_address: &str,
        chain_codes: Vec<String>,
        is_multisig: Option<bool>,
    ) -> Result<Vec<AssetsEntity>, crate::ServiceError> {
        let tx = repo;

        let accounts = tx
            .account_list_by_wallet_address_and_account_id_and_chain_codes(
                Some(wallet_address),
                Some(account_id),
                chain_codes,
            )
            .await?;
        let addresses = accounts.into_iter().map(|info| info.address).collect();
        let data = tx.get_coin_assets_in_address(addresses).await?;
        if let Some(is_multisig) = is_multisig {
            if is_multisig {
                return Ok(data
                    .into_iter()
                    .filter(|val| val.is_multisig == 2)
                    .collect());
            } else {
                return Ok(data
                    .into_iter()
                    .filter(|val| val.is_multisig == 0 || val.is_multisig == 1)
                    .collect());
            }
        }
        Ok(data)
    }

    pub async fn get_local_coin_list(
        &self,
        repo: &mut ResourcesRepo,
        addresses: Vec<String>,
        chain_code: Option<String>,
        keyword: Option<&str>,
        is_multisig: Option<bool>,
    ) -> Result<crate::response_vo::coin::CoinInfoList, crate::ServiceError> {
        let tx = repo;

        let _is_multisig = if let Some(is_multisig) = is_multisig
            && !is_multisig
        {
            None
        } else {
            is_multisig
        };

        let assets_list = tx
            .lists(addresses, chain_code, keyword, _is_multisig)
            .await
            .map_err(crate::ServiceError::Database)?;

        let mut res = crate::response_vo::coin::CoinInfoList::default();
        for assets in assets_list {
            let coin =
                CoinDomain::get_coin(&assets.chain_code, &assets.symbol, assets.token_address())
                    .await?;
            if let Some(info) = res
                .iter_mut()
                .find(|info| info.symbol == assets.symbol && coin.is_default == 1)
            {
                info.chain_list
                    .entry(assets.chain_code.clone())
                    .or_insert(assets.token_address);
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
                    show_contract: false,
                });
            }
        }

        Ok(res)
    }

    // keyword 存在都要展示合约地址
    // 链相同，symbol相同 大于2 显示地址
    pub async fn show_contract(
        pool: &DbPool,
        keyword: Option<&str>,
        res: &mut CoinInfoList,
    ) -> Result<(), crate::ServiceError> {
        let has_keyword = keyword.is_some();

        for coin in res.iter_mut() {
            let chain_len = coin.chain_list.len();

            if has_keyword || coin.is_default {
                // 有 keyword：只有恰好 1 条链才显示
                coin.show_contract = chain_len == 1;
                continue;
            }

            // 无 keyword 的逻辑
            match chain_len {
                1 => {
                    let chain_code = coin
                        .chain_list
                        .keys()
                        .next()
                        .expect("len()==1 已保证存在 key");

                    let same_coin_num =
                        CoinRepo::same_coin_num(pool, &coin.symbol, chain_code).await?;

                    coin.show_contract = same_coin_num > 1;
                }
                _ => {
                    // 0 或 >1 条链都不显示
                    coin.show_contract = false;
                }
            }
        }

        Ok(())
    }

    // 根据钱包地址来同步资产余额( 目前不需要在进行使用 )
    pub async fn sync_assets_by_wallet(
        wallet_address: String,
        account_id: Option<u32>,
        symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let list = AccountEntity::lists_by_wallet_address(
            &wallet_address,
            account_id,
            None,
            pool.as_ref(),
        )
        .await?;

        // 获取地址
        let addr = list
            .iter()
            .map(|a| a.address.clone())
            .collect::<Vec<String>>();

        Self::do_async_balance(pool, addr, None, symbol).await
    }

    pub async fn sync_assets_by_addr_chain(
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        Self::do_async_balance(pool, addr, chain_code, symbol).await
    }

    // 从后端同步余额(根据地址-链)
    pub async fn async_balance_from_backend_addr(
        addr: String,
        chain_code: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        // 单个地址处理
        let pool = crate::Context::get_global_sqlite_pool()?;

        let backhand = crate::Context::get_global_backend_api()?;

        // 获取这个地址对应的链码,如果未传
        let codes = if let Some(chain_code) = chain_code.clone() {
            vec![chain_code]
        } else {
            let account =
                AccountEntity::list_in_address(pool.as_ref(), &[addr.clone()], None).await?;

            account
                .iter()
                .map(|a| a.chain_code.clone())
                .collect::<Vec<String>>()
        };

        for code in codes {
            let resp = backhand.wallet_assets_chain_list(&addr, &code).await?;

            for item in resp.list.into_iter() {
                let amount = wallet_utils::unit::string_to_f64(&item.amount)?;
                if amount >= 0.0 {
                    let assets_id = AssetsId {
                        address: item.address,
                        chain_code: item.chain_code,
                        symbol: item.symbol.to_uppercase(),
                        token_address: item.contract_address,
                    };

                    let r =
                        AssetsEntity::update_balance(pool.as_ref(), &assets_id, &item.amount).await;

                    if let Err(e) = r {
                        tracing::warn!("udpate balance error {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    // 从后端同步余额(根据钱包-账号)
    pub async fn async_balance_from_backend_wallet(
        wallet_address: String,
        account_id: Option<u32>,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let wallet = WalletEntity::detail(pool.as_ref(), &wallet_address).await?;

        if let Some(wallet) = wallet {
            let backhand = crate::Context::get_global_backend_api()?;

            // 本地的index 进行了 + 1
            let index = account_id.map(|x| x - 1);
            let resp = backhand.wallet_assets_list(wallet.uid, index).await?;

            tracing::warn!("resp = {:#?}", resp);
            for item in resp.list.into_iter() {
                let amount = wallet_utils::unit::string_to_f64(&item.amount)?;
                if amount >= 0.0 {
                    let assets_id = AssetsId {
                        address: item.address,
                        chain_code: item.chain_code,
                        symbol: item.symbol.to_uppercase(),
                        token_address: item.contract_address,
                    };

                    let r =
                        AssetsEntity::update_balance(pool.as_ref(), &assets_id, &item.amount).await;

                    if let Err(e) = r {
                        tracing::warn!("udpate balance error {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn do_async_balance(
        pool: DbPool,
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        let mut assets =
            AssetsEntity::all_assets(pool.as_ref(), addr, chain_code, None, None).await?;
        if !symbol.is_empty() {
            assets.retain(|asset| symbol.contains(&asset.symbol));
        }

        let results = ChainBalance::sync_address_balance(&assets).await?;

        for (assets_id, balance) in &results {
            if let Err(e) = AssetsEntity::update_balance(pool.as_ref(), assets_id, balance).await {
                tracing::error!("更新余额出错: {}", e);
            }
        }

        Ok(())
    }

    pub(crate) async fn init_default_assets(
        coins: &[CoinEntity],
        address: &str,
        chain_code: &str,
        req: &mut TokenQueryPriceReq,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        for coin in coins {
            if chain_code == coin.chain_code {
                let assets_id = AssetsId::new(
                    address,
                    &coin.chain_code,
                    &coin.symbol,
                    coin.token_address(),
                );
                let assets =
                    CreateAssetsVo::new(assets_id, coin.decimals, coin.protocol.clone(), 0)
                        .with_name(&coin.name)
                        .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                if coin.price.is_empty() {
                    req.insert(
                        chain_code,
                        &assets.assets_id.token_address.clone().unwrap_or_default(),
                    );
                }
                AssetsEntity::upsert_assets(&*pool, assets).await?;
            }
        }
        Ok(())
    }

    // 根据地址和链初始化多签账号里面的资产
    // address :multisig account address ,
    pub async fn init_default_multisig_assets(
        address: String,
        chain_code: String,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let default_coins =
            CoinEntity::list_v2(&*pool, None, Some(chain_code.clone()), Some(1)).await?;
        let mut symbols = Vec::new();
        for coin in default_coins {
            let assets_id =
                AssetsId::new(&address, &chain_code, &coin.symbol, coin.token_address());
            let assets = CreateAssetsVo::new(
                assets_id,
                coin.decimals,
                coin.protocol.clone(),
                CoinMultisigStatus::IsMultisig.to_i8() as i32,
            )
            .with_name(&coin.name)
            .with_u256(alloy::primitives::U256::default(), coin.decimals)?;

            AssetsEntity::upsert_assets(&*pool, assets).await?;
            symbols.push(coin.symbol);
        }

        // 同步资产余额
        AssetsDomain::sync_assets_by_addr_chain(vec![address], Some(chain_code), symbols).await?;
        Ok(())
    }

    // swap 增加本地不存在的资产
    pub async fn swap_sync_assets(
        token: SwapTokenInfo,
        recipient: String,
        chain_code: String,
    ) -> Result<(), crate::ServiceError> {
        // notes 不能更新币价
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        // let time = wallet_utils::time::now();
        let coin = CoinRepo::coin_by_chain_address(&chain_code, &token.token_addr, &pool).await?;
        // let coin_data = CoinData::new(
        //     Some(token.symbol.clone()),
        //     &token.symbol,
        //     &chain_code,
        //     Some(token.token_addr.clone()),
        //     Some("0".to_string()),
        //     None,
        //     token.decimals as u8,
        //     0,
        //     0,
        //     1,
        //     true,
        //     time,
        //     time,
        // );
        // if let Err(e) = CoinEntity::upsert_multi_coin(pool.as_ref(), vec![coin_data]).await {
        //     tracing::error!("swap insert coin faild : {}", e);
        // };

        // 资产是否存在不存在新增
        let assets_id = AssetsId::new(
            &recipient,
            &chain_code,
            &token.symbol,
            Some(token.token_addr),
        );
        let assets =
            CreateAssetsVo::new(assets_id, token.decimals as u8, None, 0).with_name(&coin.name);

        if let Err(e) = AssetsEntity::upsert_assets(pool.as_ref(), assets).await {
            tracing::error!("swap insert assets faild : {}", e);
        };

        Ok(())
    }
}

struct BalanceTask {
    address: String,
    chain_code: String,
    symbol: String,
    decimals: u8,
    token_address: Option<String>,
}
struct ChainBalance;
impl ChainBalance {
    async fn sync_address_balance(
        assets: &[AssetsEntity],
    ) -> Result<Vec<(AssetsId, String)>, crate::ServiceError> {
        // 限制最大并发数为 10
        let sem = Arc::new(Semaphore::new(10));
        let mut tasks = vec![];

        for asset in assets.iter() {
            let bal = BalanceTask {
                address: asset.address.clone(),
                chain_code: asset.chain_code.clone(),
                symbol: asset.symbol.clone(),
                decimals: asset.decimals,
                token_address: asset.token_address(),
            };
            tasks.push(bal);
        }

        // 并发获取余额并格式化
        let results = stream::iter(tasks)
            .map(|task| Self::fetch_balance(task, sem.clone()))
            .buffer_unordered(10)
            .filter_map(|x| async move { x })
            .collect::<Vec<_>>()
            .await;

        Ok(results)
    }

    // 从任务获取余额并返回结果
    async fn fetch_balance(task: BalanceTask, sem: Arc<Semaphore>) -> Option<(AssetsId, String)> {
        // 获取并发许可
        let _permit = sem.acquire().await.ok()?;

        // 获取适配器
        let adapter = ChainAdapterFactory::get_transaction_adapter(&task.chain_code)
            .await
            .map_err(|e| {
                tracing::error!("获取链详情出错: {}，链代码: {}", e, task.chain_code.clone())
            })
            .ok()?;

        // 获取余额
        let raw = adapter
            .balance(&task.address, task.token_address.clone())
            .await
            .map_err(|e| {
                tracing::error!(
                    "获取余额出错: 地址={}, 链={}, 符号={}, token={:?}, 错误={}",
                    task.address,
                    task.chain_code,
                    task.symbol,
                    task.token_address,
                    e
                )
            })
            .ok()?;

        // 格式化
        let bal_str = wallet_utils::unit::format_to_string(raw, task.decimals)
            .unwrap_or_else(|_| "0".to_string());

        // 构建 ID
        let id = AssetsId {
            address: task.address,
            chain_code: task.chain_code,
            symbol: task.symbol,
            token_address: task.token_address,
        };

        Some((id, bal_str))
    }
}
