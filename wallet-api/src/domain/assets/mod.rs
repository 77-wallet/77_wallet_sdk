use super::chain::adapter::ChainAdapterFactory;
use crate::{
    infrastructure::task_queue::{
        task::{Task, Tasks},
        CommonTask,
    },
    request::transaction::SwapTokenInfo,
};
use futures::{stream, StreamExt};
use std::sync::Arc;
use tokio::sync::Semaphore;
use wallet_database::{
    dao::assets::CreateAssetsVo,
    entities::{
        account::AccountEntity,
        assets::{AssetsEntity, AssetsId},
        coin::{CoinData, CoinEntity, CoinMultisigStatus},
        wallet::WalletEntity,
    },
    repositories::{account::AccountRepoTrait, assets::AssetsRepoTrait, ResourcesRepo},
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

    pub async fn upsert_assets(
        &mut self,
        repo: &mut ResourcesRepo,
        mut req: TokenQueryPriceReq,
        address: &str,
        chain_code: &str,
        symbol: &str,
        decimals: u8,
        token_address: Option<String>,
        protocol: Option<String>,
        is_multisig: i32,
        name: &str,
        price: &str,
    ) -> Result<(), crate::ServiceError> {
        let tx = repo;
        let assets_id =
            wallet_database::entities::assets::AssetsId::new(address, chain_code, symbol);

        let assets = wallet_database::dao::assets::CreateAssetsVo::new(
            assets_id,
            decimals,
            token_address.clone(),
            protocol.clone(),
            is_multisig,
        )
        .with_name(name)
        .with_u256(alloy::primitives::U256::default(), decimals)?;

        if price.is_empty() {
            req.insert(
                chain_code,
                &assets.token_address.clone().unwrap_or_default(),
            );
            let task = Task::Common(CommonTask::QueryCoinPrice(req));
            Tasks::new().push(task).send().await?;
        }
        tx.upsert_assets(assets).await?;
        Ok(())
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

        let coin_list = tx
            .lists(addresses, chain_code, keyword, _is_multisig)
            .await
            .map_err(crate::ServiceError::Database)?;
        let mut res = crate::response_vo::coin::CoinInfoList::default();
        for coin in coin_list {
            if let Some(info) = res.iter_mut().find(|info| info.symbol == coin.symbol) {
                info.chain_list.insert(crate::response_vo::coin::ChainInfo {
                    chain_code: coin.chain_code,
                    token_address: Some(coin.token_address),
                    protocol: coin.protocol,
                });
            } else {
                res.push(crate::response_vo::coin::CoinInfo {
                    symbol: coin.symbol,
                    name: Some(coin.name),
                    chain_list: std::collections::HashSet::from([
                        crate::response_vo::coin::ChainInfo {
                            chain_code: coin.chain_code,
                            token_address: Some(coin.token_address),
                            protocol: coin.protocol,
                        },
                    ]),
                    is_multichain: false,
                });
            }
        }

        Ok(res)
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
        tx: &mut ResourcesRepo,
    ) -> Result<(), crate::ServiceError> {
        for coin in coins {
            if chain_code == coin.chain_code {
                let assets_id = AssetsId::new(address, &coin.chain_code, &coin.symbol);
                let assets = CreateAssetsVo::new(
                    assets_id,
                    coin.decimals,
                    coin.token_address.clone(),
                    coin.protocol.clone(),
                    0,
                )
                .with_name(&coin.name)
                .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                if coin.price.is_empty() {
                    req.insert(
                        chain_code,
                        &assets.token_address.clone().unwrap_or_default(),
                    );
                }
                tx.upsert_assets(assets).await?;
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
            CoinEntity::list(&*pool, &[], Some(chain_code.clone()), Some(1)).await?;
        let mut symbols = Vec::new();
        for coin in default_coins {
            let assets_id = AssetsId::new(&address, &chain_code, &coin.symbol);
            let assets = CreateAssetsVo::new(
                assets_id,
                coin.decimals,
                coin.token_address.clone(),
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
        let time = wallet_utils::time::now();
        let coin_data = CoinData::new(
            Some(token.symbol.clone()),
            &token.symbol,
            &chain_code,
            Some(token.token_addr.clone()),
            Some("0".to_string()),
            None,
            token.decimals as u8,
            0,
            0,
            1,
            time,
            time,
        );
        if let Err(e) = CoinEntity::upsert_multi_coin(pool.as_ref(), vec![coin_data]).await {
            tracing::error!("swap insert coin faild : {}", e);
        };

        // 资产是否存在不存在新增
        let assets_id = AssetsId::new(&recipient, &chain_code, &token.symbol);
        let assets = CreateAssetsVo::new(
            assets_id,
            token.decimals as u8,
            Some(token.token_addr),
            None,
            0,
        );

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
        };

        Some((id, bal_str))
    }
}
