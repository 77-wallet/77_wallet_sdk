use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    domain::app::config::ConfigDomain,
    error::system::SystemError,
    messaging::notify::{
        FrontendNotifyEvent,
        api_wallet::{ApiWalletSyncAccountBalanceMsgFrontItem, ApiWalletSyncAssetsMsgFront},
        event::NotifyEvent,
    },
    response_vo::coin::{TokenCurrencies, TokenCurrencyId},
};
use dashmap::{DashMap, DashSet};
use once_cell::sync::Lazy;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use sqlx::SqlitePool;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, error, warn};
use wallet_database::{
    entities::{
        api_assets::{ApiAssetsEntity, AssetWithWalletAddress},
        assets::AssetsIdVo,
    },
    repositories::{api_wallet::assets::ApiAssetsRepo, exchange_rate::ExchangeRateRepo},
};

use crate::{error::service::ServiceError, response_vo::account::BalanceInfo};

#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub wallet_address: String,
    pub address: String,
    pub symbol: String,
    pub chain_code: String,
    pub token_address: String,
    pub balance: Decimal,
    pub decimals: u8,
}

// 定义消息类型
enum AssetCalcMessage {
    // 资产更新消息
    AssetUpdate {
        wallet_address: String,
        address: String,
        chain_code: String,
        token_address: String,
        response_tx: mpsc::Sender<Result<(), ServiceError>>,
    },
    // 价格更新消息
    PriceUpdate {
        symbol: String,
        chain_code: String,
        token_address: Option<String>,
        price_real: f64,
        response_tx: mpsc::Sender<Result<(), ServiceError>>,
    },
    // 获取钱包余额
    GetWalletBalance {
        response_tx: mpsc::Sender<Result<HashMap<String, BalanceInfo>, ServiceError>>,
    },
    // 获取账户余额
    GetAccountBalance {
        wallet_address: String,
        chain_code: Option<String>,
        response_tx: mpsc::Sender<Result<HashMap<String, BalanceInfo>, ServiceError>>,
    },
    // 获取余额摘要
    GetBalanceSummary {
        wallet_address: Option<String>,
        account_id: Option<u32>,
        chain_code: Option<String>,
        response_tx: mpsc::Sender<Result<BalanceInfo, ServiceError>>,
    },
    // 初始化账户缓存
    InitAccountCache {
        response_tx: mpsc::Sender<Result<(), ServiceError>>,
    },
    // 添加账户到缓存
    AddAccountToCache {
        address: String,
        account_id: u32,
        wallet: String,
        response_tx: mpsc::Sender<()>,
    },
    // 获取总价值
    GetTotalUsdt {
        response_tx: mpsc::Sender<Decimal>,
    },
    // 启动批处理任务
    StartBatchRecalculator {
        interval_ms: u64,
        response_tx: mpsc::Sender<Result<(), ServiceError>>,
    },
    SendAffectedAccounts {
        assets: Vec<AssetEntry>,
    },
    AggregateAndNotify {
        assets: Vec<AssetEntry>,
        token_currencies: TokenCurrencies,
        currency: String,
    },
}

// Actor状态结构
pub struct AssetCalcState {
    // 数据库连接池
    pool: Arc<SqlitePool>,

    // 地址到钱包的映射
    address_to_wallet: HashMap<String, String>,
    // 地址到账户ID的映射
    pub(crate) address_to_account_id: HashMap<String, u32>,

    // 账户价值缓存
    account_value_cache: DashMap<(String, String), Decimal>,
    // 资产价值缓存
    asset_value_cache: DashMap<AssetKey, BalanceInfo>,

    // 汇率缓存
    exchange_rate_cache: ExchangeRateCache,

    // Token货币信息
    token_currencies: TokenCurrencies,

    // 脏价格集合
    dirty_price_set: DashSet<TokenCurrencyId>,
    // 脏资产集合
    dirty_asset_set: DashSet<AssetKey>,

    // 总价值
    total_usdt: Decimal,

    // 缓存更新时间
    last_cache_update: Instant,
    // 缓存条目过期时间
    cache_ttl: Duration,
}

impl AssetCalcState {
    // 初始化状态
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            address_to_wallet: HashMap::new(),
            address_to_account_id: HashMap::new(),
            account_value_cache: DashMap::new(),
            asset_value_cache: DashMap::new(),
            exchange_rate_cache: ExchangeRateCache {
                rates: HashMap::new(),
                last_updated: Instant::now() - Duration::from_secs(60 * 6), // 初始设置为过期
            },
            token_currencies: TokenCurrencies::default(),
            dirty_price_set: DashSet::new(),
            dirty_asset_set: DashSet::new(),
            total_usdt: Decimal::ZERO,
            last_cache_update: Instant::now(),
            cache_ttl: Duration::from_secs(5 * 60), // 默认5分钟过期
        }
    }

    // 检查缓存是否过期
    pub fn is_cache_expired(&self) -> bool {
        Instant::now() - self.last_cache_update > self.cache_ttl
    }

    // 更新缓存时间
    pub fn update_cache_timestamp(&mut self) {
        self.last_cache_update = Instant::now();
    }

    // 清除过期的缓存项
    pub fn cleanup_expired_cache(&mut self) {
        // 这里可以实现基于时间的缓存清理逻辑
        // 目前我们使用简单的全局过期策略
        if self.is_cache_expired() {
            // 清除部分或全部缓存
            // 注意：实际应用中，可能需要更精细的过期策略
            debug!("Cache expired, cleaning up asset_value_cache entries");
            // 为避免过度清理，我们可以选择性地只清理部分过期项
            // 例如，对于很少访问的地址，可以优先清理
        }
    }

    // 批量更新资产值缓存
    pub fn batch_update_asset_cache(&mut self, updates: Vec<(AssetKey, BalanceInfo)>) {
        let mut total_usdt_sum = Decimal::ZERO;

        for (key, value) in updates {
            // 更新缓存
            self.asset_value_cache.insert(key, value);
        }

        // 重新计算总USDT价值
        for entry in self.asset_value_cache.iter() {
            if let Some(fiat_value) = entry.value().fiat_value {
                total_usdt_sum += Decimal::new((fiat_value * 100.0) as i64, 2);
            }
        }

        // 更新总USDT价值
        self.total_usdt = total_usdt_sum;
        // 更新缓存时间戳
        self.update_cache_timestamp();
    }

    // 标记资产为脏数据
    pub fn mark_asset_dirty(
        &mut self,
        wallet_address: &str,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) {
        let key = AssetKey::new(wallet_address, address, chain_code, token_address);
        self.dirty_asset_set.insert(key);
    }

    // 标记价格为脏数据
    pub fn mark_price_dirty(
        &mut self,
        symbol: &str,
        chain_code: &str,
        token_address: &Option<String>,
    ) {
        let id = TokenCurrencyId::new(symbol, chain_code, token_address.clone());
        self.dirty_price_set.insert(id);
    }
}

// 汇率缓存结构
#[derive(Clone, Debug)]
pub struct ExchangeRateCache {
    pub rates: HashMap<String, f64>,
    pub last_updated: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub wallet_address: String,
    pub address: String,
    pub chain_code: String,
    pub token_address: String,
}

impl AssetKey {
    fn new(wallet_address: &str, address: &str, chain_code: &str, token_address: &str) -> AssetKey {
        AssetKey {
            wallet_address: wallet_address.to_string(),
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            token_address: token_address.to_string(),
        }
    }

    // 生成用于数据库查询的键
    pub fn gen_key(&self) -> String {
        format!("{}:{}:{}", self.address, self.chain_code, self.token_address)
    }
}

// AssetCalcActor结构体
struct AssetCalcActor {
    state: AssetCalcState,
    receiver: mpsc::Receiver<AssetCalcMessage>,
}

impl AssetCalcActor {
    // 创建新的Actor
    fn new(pool: Arc<SqlitePool>, receiver: mpsc::Receiver<AssetCalcMessage>) -> Self {
        let state = AssetCalcState {
            pool,
            address_to_wallet: HashMap::new(),
            address_to_account_id: HashMap::new(),
            account_value_cache: DashMap::new(),
            asset_value_cache: DashMap::new(),
            exchange_rate_cache: ExchangeRateCache {
                rates: HashMap::new(),
                last_updated: Instant::now() - Duration::from_secs(60 * 6),
            },
            token_currencies: TokenCurrencies::default(),
            dirty_price_set: DashSet::new(),
            dirty_asset_set: DashSet::new(),
            total_usdt: Decimal::ZERO,
            last_cache_update: Instant::now(),
            cache_ttl: Duration::from_secs(5 * 60),
        };

        Self { state: state, receiver }
    }

    // 启动Actor处理循环
    async fn run(mut self) {
        debug!("AssetCalcActor started");

        // 初始化账户缓存
        if let Err(e) = self.handle_init_account_cache().await {
            error!("Failed to initialize account cache: {:?}", e);
        }

        // 初始刷新缓存
        let pool = &self.state.pool;

        if let Err(e) = Self::refresh_all_caches(pool, &mut self.state).await {
            error!("Failed to perform initial cache refresh: {:?}", e);
        }

        while let Some(msg) = self.receiver.recv().await {
            match msg {
                AssetCalcMessage::AssetUpdate {
                    wallet_address,
                    address,
                    chain_code,
                    token_address,
                    mut response_tx,
                } => {
                    let result = self
                        .handle_asset_update(&wallet_address, &address, &chain_code, &token_address)
                        .await;

                    // 如果更新成功，触发异步更新
                    if result.is_ok() {
                        let pool_clone = { Arc::clone(pool) };

                        tokio::spawn(async move {
                            // 异步触发更新
                            if let Err(e) =
                                Self::process_batch_updates(&state_clone, &pool_clone).await
                            {
                                error!(
                                    "Failed to process batch update after asset change: {:?}",
                                    e
                                );
                            }
                        });
                    }

                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::PriceUpdate {
                    symbol,
                    chain_code,
                    token_address,
                    price_real,
                    mut response_tx,
                } => {
                    let result = self
                        .handle_price_update(&symbol, &chain_code, &token_address, price_real)
                        .await;

                    // 如果更新成功，触发异步更新
                    if result.is_ok() {
                        let pool_clone = self.state.pool.clone();

                        tokio::spawn(async move {
                            // 异步触发更新
                            if let Err(e) =
                                Self::process_batch_updates(&state_clone, &pool_clone).await
                            {
                                error!(
                                    "Failed to process batch update after price change: {:?}",
                                    e
                                );
                            }
                        });
                    }

                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::GetWalletBalance { mut response_tx } => {
                    // 检查缓存是否过期，如果过期则刷新
                    self.ensure_cache_fresh().await;

                    let result = self.handle_get_wallet_balance().await;
                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::GetAccountBalance {
                    wallet_address,
                    chain_code,
                    mut response_tx,
                } => {
                    // 检查缓存是否过期，如果过期则刷新
                    self.ensure_cache_fresh().await;

                    let result =
                        self.handle_get_account_balance(&wallet_address, chain_code.clone()).await;
                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::GetBalanceSummary {
                    wallet_address,
                    account_id,
                    chain_code,
                    mut response_tx,
                } => {
                    tracing::info!("ensure_cache_fresh");
                    // 检查缓存是否过期，如果过期则刷新
                    self.ensure_cache_fresh().await;
                    tracing::info!("ensure_cache_fresh end");

                    let result = self
                        .handle_get_balance_summary(wallet_address.clone(), account_id, chain_code)
                        .await;
                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::InitAccountCache { mut response_tx } => {
                    let result = self.handle_init_account_cache().await;

                    // 如果初始化成功，触发缓存刷新
                    if result.is_ok() {
                        let pool_clone = self.state.pool.clone();

                        tokio::spawn(async move {
                            if let Err(e) =
                                Self::refresh_all_caches(&pool_clone, &state_clone).await
                            {
                                error!(
                                    "Failed to refresh cache after account cache initialization: {:?}",
                                    e
                                );
                            }
                        });
                    }

                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::AddAccountToCache {
                    address,
                    account_id,
                    wallet,
                    mut response_tx,
                } => {
                    self.handle_add_account_to_cache(&address, account_id, &wallet).await;

                    // 异步刷新该账户的资产
                    let address_clone = address.clone();
                    let pool_clone = self.state.pool.clone();

                    tokio::spawn(async move {
                        // 获取该账户的所有资产
                        match ApiAssetsRepo::get_api_assets_by_address(
                            &pool_clone,
                            vec![address_clone],
                            None,
                        )
                        .await
                        {
                            Ok(assets) => {
                                // 标记所有相关资产为脏
                                {
                                    let mut state_write = state_clone.write().await;
                                    for asset in assets {
                                        state_write.mark_asset_dirty(
                                            &wallet,
                                            &asset.address,
                                            &asset.chain_code,
                                            &asset.token_address,
                                        );
                                    }
                                }

                                // 触发更新
                                if let Err(e) =
                                    Self::process_batch_updates(&state_clone, &pool_clone).await
                                {
                                    error!(
                                        "Failed to process batch update after adding account: {:?}",
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                error!("Failed to get assets for new account: {:?}", e);
                            }
                        }
                    });

                    let _ = response_tx.send(()).await;
                }
                AssetCalcMessage::GetTotalUsdt { mut response_tx } => {
                    // 检查缓存是否过期，如果过期则刷新
                    self.ensure_cache_fresh().await;

                    let result = self.handle_get_total_usdt().await;
                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::StartBatchRecalculator { interval_ms, mut response_tx } => {
                    let result = self.handle_start_batch_recalculator(interval_ms).await;
                    let _ = response_tx.send(result).await;
                }
                AssetCalcMessage::SendAffectedAccounts { assets } => {
                    self.handle_send_affected_accounts(assets).await;
                }
                AssetCalcMessage::AggregateAndNotify { assets, token_currencies, currency } => {
                    self.handle_aggregate_and_notify(&assets, token_currencies, currency).await;
                }
            }
        }

        debug!("AssetCalcActor stopped");
    }

    // 确保缓存新鲜
    async fn ensure_cache_fresh(&self) {
        // 检查缓存是否过期
        if self.state.is_cache_expired() {
            // 异步刷新缓存
            let pool_clone = self.state.pool.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::refresh_all_caches(&pool_clone, &state_clone).await {
                    error!("Failed to refresh expired cache: {:?}", e);
                }
            });
        }
    }

    async fn handle_send_affected_accounts(&self, assets: Vec<AssetEntry>) {
        // 按账户级别聚合
        let mut affected_accounts: HashSet<(String, u32)> = HashSet::new();

        {
            let map = self.state.address_to_account_id.clone();

            for a in &assets {
                if let Some(account_id) = map.get(&a.address) {
                    affected_accounts.insert((a.wallet_address.clone(), *account_id));
                }
            }
        }
        // 过滤未变化账户
        let changed_accounts = ApiWalletSyncAssetsMsgFront::new();
        for (ref wallet_address, account_id) in affected_accounts {
            let balance_info = self
                .handle_get_balance_summary(
                    Some(wallet_address.to_string()),
                    Some(account_id),
                    None,
                )
                .await
                .unwrap();
            changed_accounts.add_item(
                wallet_address,
                ApiWalletSyncAccountBalanceMsgFrontItem::new(account_id, balance_info),
            );
        }

        // 只有有变化才推送
        if let Err(e) = FrontendNotifyEvent::new(NotifyEvent::ApiWalletSyncAssets(changed_accounts))
            .send()
            .await
        {
            tracing::error!("send error: {}", e);
        }
    }

    async fn handle_aggregate_and_notify(
        &self,
        assets: &[AssetEntry],
        token_currencies_snapshot: TokenCurrencies,
        currency: String,
    ) {
        // 使用标准迭代器替代并行迭代，确保线程安全
        for a in assets {
            let asset_key =
                AssetKey::new(&a.wallet_address, &a.address, &a.chain_code, &a.token_address);

            // 改进错误处理，避免使用unwrap_or掩盖错误
            let balance_info = match token_currencies_snapshot.calculate_sync_to_balance(
                &currency,
                &a.balance.to_string(),
                &a.symbol,
                &a.chain_code,
                Some(a.token_address.clone()),
            ) {
                Ok(balance_info) => balance_info,
                Err(e) => {
                    tracing::error!(
                        "Failed to calculate balance for asset: address={}, symbol={}, error: {:?}",
                        a.address,
                        a.symbol,
                        e
                    );
                    // 使用合理的默认值
                    BalanceInfo {
                        amount: 0.0,
                        currency: currency.clone(),
                        unit_price: None,
                        fiat_value: Some(0.0),
                    }
                }
            };

            // 先检查是否存在旧值，用于后续正确更新TOTAL_USDT
            let old_fiat_value =
                self.state.asset_value_cache.get(&asset_key).and_then(|old| old.fiat_value).map(
                    |v| {
                        // 直接使用v作为浮点数进行计算
                        Decimal::new((v * 100.0) as i64, 2)
                    },
                );

            // 更新资产缓存
            self.state.asset_value_cache.insert(asset_key.clone(), balance_info.clone());

            // 正确更新TOTAL_USDT：先减去旧值，再加上新值
            // self.update_total_usdt(old_fiat_value, balance_info.fiat_value).await;
        }
    }

    // 处理资产更新
    async fn handle_asset_update(
        &mut self,
        wallet_address: &str,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), ServiceError> {
        if wallet_address.is_empty() || address.is_empty() || chain_code.is_empty() {
            error!(
                "Invalid input for asset update: wallet_address={}, address={}, chain_code={}",
                wallet_address, address, chain_code
            );
            return Err(ServiceError::Parameter("Invalid input parameters".to_string()));
        }

        // 确保地址到钱包的映射存在
        if !self.state.address_to_wallet.contains_key(address) {
            self.state.address_to_wallet.insert(address.to_string(), wallet_address.to_string());
            debug!("Added address-to-wallet mapping: {} -> {}", address, wallet_address);
        }

        // 标记资产为脏
        self.state.mark_asset_dirty(wallet_address, address, chain_code, token_address);

        Ok(())
    }

    // 处理价格更新
    async fn handle_price_update(
        &mut self,
        symbol: &str,
        chain_code: &str,
        token_address: &Option<String>,
        price_real: f64,
    ) -> Result<(), ServiceError> {
        let currency = ConfigDomain::get_currency().await?;

        // 更新token价格
        let (fiat_price, rate) = {
            // 获取汇率
            let exchange_rates = self.get_exchange_rates().await?;

            if let Some(rate_value) = exchange_rates.get(&currency) {
                // 使用Decimal进行精确计算
                let price_decimal = Decimal::new((price_real * 100.0) as i64, 2);
                let rate_decimal = Decimal::new((*rate_value * 100.0) as i64, 2);
                let fiat_price_decimal = price_decimal * rate_decimal;

                (fiat_price_decimal.to_f64(), *rate_value)
            } else {
                (None, 1.0)
            }
        };

        // 标记价格为脏并更新价格信息
        self.state.mark_price_dirty(symbol, chain_code, token_address);

        // 更新token价格
        let id = TokenCurrencyId::new(symbol, chain_code, token_address.clone());
        self.state
            .token_currencies
            .entry(id)
            .and_modify(|entry| {
                entry.price = Some(price_real);
                entry.currency_price = fiat_price;
                entry.rate = rate;
            })
            .or_insert_with(|| {
                use wallet_transport_backend::response_vo::coin::TokenCurrency;
                TokenCurrency::new(chain_code, symbol, "", Some(price_real), fiat_price, rate)
            });

        debug!(
            "update_token_price symbol: {}, chain_code: {}, token_address: {:?}, price_real: {}, rate: {}",
            symbol, chain_code, token_address, price_real, rate
        );

        Ok(())
    }

    // 获取汇率（带缓存）
    async fn get_exchange_rates(&mut self) -> Result<HashMap<String, f64>, ServiceError> {
        const CACHE_DURATION: Duration = Duration::from_secs(60 * 5);

        // 检查缓存是否过期
        if Instant::now() - self.state.exchange_rate_cache.last_updated < CACHE_DURATION {
            return Ok(self.state.exchange_rate_cache.rates.clone());
        }

        // 缓存过期，重新从数据库加载
        let exchange_rate_list = ExchangeRateRepo::list(&self.state.pool).await?;

        // 更新缓存
        let mut rates = HashMap::new();
        for rate in exchange_rate_list {
            rates.insert(rate.target_currency.clone(), rate.rate);
        }

        self.state.exchange_rate_cache.rates = rates.clone();
        self.state.exchange_rate_cache.last_updated = Instant::now();

        Ok(rates)
    }

    // 处理获取钱包余额
    async fn handle_get_wallet_balance(
        &mut self,
    ) -> Result<HashMap<String, BalanceInfo>, ServiceError> {
        let mut wallet_totals: HashMap<String, BalanceInfo> = HashMap::new();

        for entry in self.state.asset_value_cache.iter() {
            if let Some(wallet_address) = self.state.address_to_wallet.get(&entry.key().address) {
                let entry_value = entry.value();
                wallet_totals
                    .entry(wallet_address.clone())
                    .and_modify(|total| {
                        total.amount_add(entry_value.amount);
                        total.fiat_add(entry_value.fiat_value);
                    })
                    .or_insert_with(|| entry_value.clone());
            }
        }

        Ok(wallet_totals)
    }

    // 处理获取账户余额
    async fn handle_get_account_balance(
        &self,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> Result<HashMap<String, BalanceInfo>, ServiceError> {
        let account_addresses: Vec<String> = self
            .state
            .address_to_wallet
            .iter()
            .filter_map(
                |(addr, wallet)| {
                    if wallet == wallet_address { Some(addr.clone()) } else { None }
                },
            )
            .collect();

        if account_addresses.is_empty() {
            return Ok(HashMap::new());
        }

        let mut account_totals: HashMap<String, BalanceInfo> = HashMap::new();
        for entry in self.state.asset_value_cache.iter() {
            let address = &entry.key().address;
            let asset_chain_code = &entry.key().chain_code;

            let chain_match = match chain_code {
                Some(ref code) => asset_chain_code == code,
                None => true,
            };

            if chain_match && account_addresses.contains(&address.to_string()) {
                let entry_value = entry.value();
                account_totals
                    .entry(address.to_string())
                    .and_modify(|total| {
                        total.amount_add(entry_value.amount);
                        total.fiat_add(entry_value.fiat_value);
                    })
                    .or_insert_with(|| entry_value.clone());
            }
        }

        Ok(account_totals)
    }

    // 处理获取余额摘要
    async fn handle_get_balance_summary(
        &self,
        wallet_address: Option<String>,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<BalanceInfo, ServiceError> {
        let mut total = BalanceInfo::default();

        // 根据参数筛选目标地址集合
        let mut target_addresses: Vec<String> = Vec::new();

        match (wallet_address, account_id) {
            (None, None) => {
                // 全部账户
                target_addresses = self.state.address_to_wallet.keys().cloned().collect();
            }
            (Some(wallet), None) => {
                // 指定钱包下所有账户
                target_addresses = self
                    .state
                    .address_to_wallet
                    .iter()
                    .filter_map(|(addr, w)| if w == &wallet { Some(addr.clone()) } else { None })
                    .collect();
            }
            (Some(wallet), Some(id)) => {
                // 指定钱包 + 账户
                target_addresses = self
                    .state
                    .address_to_account_id
                    .iter()
                    .filter_map(|(addr, aid)| {
                        if *aid == id
                            && self
                                .state
                                .address_to_wallet
                                .get(addr)
                                .map(|w| w == &wallet)
                                .unwrap_or(false)
                        {
                            Some(addr.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
            }
            _ => {
                return Ok(total);
            }
        }

        if target_addresses.is_empty() {
            return Ok(total);
        }

        // 遍历缓存，按条件聚合
        for entry in self.state.asset_value_cache.iter() {
            let address = &entry.key().address;
            let asset_chain_code = &entry.key().chain_code;

            // 筛选地址
            if !target_addresses.contains(address) {
                continue;
            }

            // 筛选链
            if let Some(chain_filter) = chain_code.as_ref() {
                if asset_chain_code != chain_filter {
                    continue;
                }
            }

            // 累加金额
            let entry_value = entry.value();
            total.amount_add(entry_value.amount);
            total.fiat_add(entry_value.fiat_value);
        }

        Ok(total)
    }

    // 处理初始化账户缓存
    async fn handle_init_account_cache(&mut self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet_map =
            wallet_database::repositories::api_wallet::account::ApiAccountRepo::account_to_wallet(
                &pool,
            )
            .await?;
        let account_list =
            wallet_database::repositories::api_wallet::account::ApiAccountRepo::list(&pool).await?;

        // 更新地址到钱包的映射
        self.state.address_to_wallet.clear();
        for wallet in wallet_map {
            self.state
                .address_to_wallet
                .insert(wallet.address.clone(), wallet.wallet_address.clone());
        }

        // 更新地址到账户ID的映射
        self.state.address_to_account_id.clear();
        for row in account_list {
            self.state.address_to_account_id.insert(row.address.clone(), row.account_id);
        }

        Ok(())
    }

    // 处理添加账户到缓存
    async fn handle_add_account_to_cache(&mut self, address: &str, account_id: u32, wallet: &str) {
        self.state.address_to_account_id.insert(address.to_string(), account_id);
        self.state.address_to_wallet.insert(address.to_string(), wallet.to_string());
    }

    // 处理获取总价值
    async fn handle_get_total_usdt(&self) -> Decimal {
        self.state.total_usdt
    }

    // 处理启动批处理任务
    async fn handle_start_batch_recalculator(&self, interval_ms: u64) -> Result<(), ServiceError> {
        let state = self.state;
        let pool = Arc::clone(&state.pool);

        tokio::spawn(async move {
            let interval = Duration::from_millis(interval_ms);
            loop {
                tokio::time::sleep(interval).await;

                if let Err(e) = Self::process_batch_updates(&state_clone, &pool).await {
                    error!("Batch update error: {:?}", e);
                }
            }
        });

        Ok(())
    }

    // 处理批处理更新
    async fn process_batch_updates(
        state: &mut AssetCalcState,
        pool: &Arc<SqlitePool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 检查缓存是否过期，过期则需要刷新整个缓存
        let need_full_refresh = state.is_cache_expired();

        // 收集脏集合
        let price_keys: Vec<TokenCurrencyId> =
            state.dirty_price_set.iter().map(|k| k.clone()).collect();
        let asset_keys: Vec<AssetKey> = state.dirty_asset_set.iter().map(|id| id.clone()).collect();

        if !need_full_refresh && price_keys.is_empty() && asset_keys.is_empty() {
            return Ok(());
        }

        debug!(
            "batch recalculation started: need_full_refresh={}, price_keys={}, asset_ids={}",
            need_full_refresh,
            price_keys.len(),
            asset_keys.len()
        );

        // 清除旧的脏标记
        state.dirty_price_set.clear();
        state.dirty_asset_set.clear();

        // 更新缓存时间戳
        state.update_cache_timestamp();

        // 克隆需要的数据，以便释放锁
        let token_currencies_clone = state.token_currencies.clone();

        if need_full_refresh {
            // 执行全量刷新
            if let Err(e) = Self::refresh_all_caches(pool, state).await {
                error!("Full cache refresh failed: {:?}", e);
            }
        } else {
            // 处理价格变更
            if !price_keys.is_empty() {
                if let Err(e) = Self::process_price_dirty_assets(
                    pool,
                    &price_keys,
                    token_currencies_clone.clone(),
                )
                .await
                {
                    error!("process_price_dirty_assets error: {:?}", e);
                }
            }

            // 处理资产变更
            if !asset_keys.is_empty() {
                if let Err(e) = Self::process_asset_dirty_assets(
                    state,
                    pool,
                    &asset_keys,
                    token_currencies_clone,
                )
                .await
                {
                    error!("process_asset_dirty_assets error: {:?}", e);
                }
            }

            // 清理过期缓存条目
            state.cleanup_expired_cache();
        }

        Ok(())
    }

    // 全量刷新缓存
    async fn refresh_all_caches(
        pool: &Arc<SqlitePool>,
        state: &mut AssetCalcState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Starting full cache refresh");

        // 获取最新资产数据 - 使用ApiAssetsRepo获取所有资产
        // 从状态中获取所有地址和pool
        let addresses: Vec<String> = state.address_to_wallet.keys().cloned().collect();

        // 使用ApiAssetsRepo::list方法获取所有资产
        let assets_list = ApiAssetsRepo::assets_with_wallet_address_by_address(pool, &addresses)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        debug!("Retrieved {} assets for full refresh", assets_list.len());

        // 获取当前价格数据
        let token_currencies_clone = state.token_currencies.clone();

        // 清空旧缓存
        state.asset_value_cache.clear();
        state.total_usdt = Decimal::ZERO;

        // 批量更新缓存
        let mut total_value = Decimal::ZERO;
        for asset in assets_list {
            let key = AssetKey::new(
                &asset.wallet_address,
                &asset.address,
                &asset.chain_code,
                &asset.token_address,
            );

            // 计算资产价值
            let token_id = TokenCurrencyId::new(
                &asset.symbol,
                &asset.chain_code,
                Some(asset.token_address.clone()),
            );
            let asset_value =
                Self::calculate_asset_value(&asset, &token_currencies_clone, &token_id)?;

            state.asset_value_cache.insert(key, asset_value.clone());

            // 更新总价值
            if let Some(fiat_value) = asset_value.fiat_value {
                total_value += Decimal::new((fiat_value * 100.0) as i64, 2);
            }
        }

        state.total_usdt = total_value;
        debug!("Full cache refresh completed: total_value={}", total_value);

        Ok(())
    }

    // 计算单个资产价值
    fn calculate_asset_value(
        asset: &AssetWithWalletAddress,
        token_currencies: &TokenCurrencies,
        token_id: &TokenCurrencyId,
    ) -> Result<BalanceInfo, crate::error::service::ServiceError> {
        let mut balance = BalanceInfo::default();
        let decimal_amount = wallet_utils::parse_func::decimal_from_str(&asset.balance)?;
        balance.amount = decimal_amount.to_f64().unwrap_or_default();

        // 尝试获取价格信息
        if let Some(token) = token_currencies.get(token_id) {
            if let Some(price) = token.price {
                // 计算法币价值
                balance.fiat_value = Some(balance.amount * price);

                // 如果有汇率信息，转换为法币
                if let Some(fiat_price) = token.currency_price {
                    balance.fiat_value = Some(balance.amount * fiat_price);
                }
            }
        }

        Ok(balance)
    }

    // 处理价格变更的资产
    async fn process_price_dirty_assets(
        pool: &Arc<SqlitePool>,
        keys: &[TokenCurrencyId],
        token_currencies: TokenCurrencies,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // process in chunks to avoid huge IN lists
        const CHUNK_KEYS: usize = 200;
        let currency = ConfigDomain::get_currency().await?;

        for chunk in keys.chunks(CHUNK_KEYS) {
            let mut keys_vec = Vec::new();
            for key in chunk {
                // 生成用于查询的键格式
                keys_vec.push(key.gen_key());
            }

            // 使用正确的Repository方法获取资产列表
            let assets_list =
                match ApiAssetsRepo::assets_with_wallet_address_by_token(pool, &keys_vec).await {
                    Ok(list) => list,
                    Err(e) => {
                        warn!("Failed to get assets by token: {:?}", e);
                        Vec::new()
                    }
                };

            let mut assets = Vec::new();
            for asset in assets_list {
                let balance = wallet_utils::parse_func::decimal_from_str(&asset.balance)?;
                let asset_entry = AssetEntry {
                    wallet_address: asset.wallet_address,
                    address: asset.address,
                    symbol: asset.symbol,
                    chain_code: asset.chain_code,
                    token_address: asset.token_address,
                    balance,
                    decimals: asset.decimals,
                };

                assets.push(asset_entry);
            }

            debug!("Processing price update for {} assets", assets.len());

            let actor_manager = crate::context::CONTEXT
                .get()
                .unwrap()
                .get_global_asset_calc_actor_manager()
                .await?;

            // 发送AggregateAndNotify消息
            if let Err(e) = actor_manager
                .sender
                .send(AssetCalcMessage::AggregateAndNotify {
                    assets: assets.clone(),
                    token_currencies: token_currencies.clone(),
                    currency: currency.clone(),
                })
                .await
            {
                warn!("Failed to send AggregateAndNotify message: {:?}", e);
            }

            // 发送SendAffectedAccounts消息
            if let Err(e) =
                actor_manager.sender.send(AssetCalcMessage::SendAffectedAccounts { assets }).await
            {
                warn!("Failed to send SendAffectedAccounts message: {:?}", e);
            }
        }

        Ok(())
    }

    // 处理资产变更
    async fn process_asset_dirty_assets(
        state: &mut AssetCalcState,
        pool: &Arc<SqlitePool>,
        keys: &[AssetKey],
        token_currencies: TokenCurrencies,
    ) -> Result<(), Box<dyn std::error::Error>> {
        const CHUNK_SIZE: usize = 200;
        let currency = ConfigDomain::get_currency().await?;

        for chunk in keys.chunks(CHUNK_SIZE) {
            let mut keys_vec = Vec::new();
            for key in chunk {
                keys_vec.push(format!("{}:{}:{}", key.address, key.chain_code, key.token_address));
            }

            // 尝试获取受影响的资产
            let assets_list =
                match ApiAssetsRepo::assets_with_wallet_address_by_address(pool, &keys_vec).await {
                    Ok(list) => list,
                    Err(e) => {
                        warn!("Failed to get assets by address: {:?}", e);
                        Vec::new()
                    }
                };

            let mut assets = Vec::new();
            for asset in assets_list {
                let balance = wallet_utils::parse_func::decimal_from_str(&asset.balance)?;
                let asset_entry = AssetEntry {
                    wallet_address: asset.wallet_address,
                    address: asset.address,
                    symbol: asset.symbol,
                    chain_code: asset.chain_code,
                    token_address: asset.token_address,
                    balance,
                    decimals: asset.decimals,
                };

                assets.push(asset_entry);
            }

            debug!("Processing asset update for {} assets", assets.len());

            // // 使用最新数据进行聚合和通知
            // asset_sync::aggregate_and_notify(&assets, token_currencies.clone(), currency.clone())
            //     .await;
            // asset_sync::affected_accounts(state, assets).await;
            // 使用AssetCalcActorManager的sender发送消息
            let actor_manager = crate::context::CONTEXT
                .get()
                .unwrap()
                .get_global_asset_calc_actor_manager()
                .await?;

            // 发送AggregateAndNotify消息
            if let Err(e) = actor_manager
                .sender
                .send(AssetCalcMessage::AggregateAndNotify {
                    assets: assets.clone(),
                    token_currencies: token_currencies.clone(),
                    currency: currency.clone(),
                })
                .await
            {
                warn!("Failed to send AggregateAndNotify message: {:?}", e);
            }

            // 发送SendAffectedAccounts消息
            if let Err(e) =
                actor_manager.sender.send(AssetCalcMessage::SendAffectedAccounts { assets }).await
            {
                warn!("Failed to send SendAffectedAccounts message: {:?}", e);
            }
        }

        Ok(())
    }
}

// AssetCalcActor的管理器
#[derive(Debug)]
pub struct AssetCalcActorManager {
    sender: mpsc::Sender<AssetCalcMessage>,
}

impl AssetCalcActorManager {
    // 创建并启动新的Actor
    pub fn start(pool: Arc<SqlitePool>) -> Self {
        let (sender, receiver) = mpsc::channel(100);

        // 启动Actor
        let actor = AssetCalcActor::new(pool, receiver);
        tokio::spawn(actor.run());

        Self { sender }
    }

    // 异步接口：资产更新
    pub async fn update_asset(
        &self,
        wallet_address: &str,
        address: &str,
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::AssetUpdate {
            wallet_address: wallet_address.to_string(),
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            token_address: token_address.to_string(),
            response_tx,
        };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：价格更新
    pub async fn update_price(
        &self,
        symbol: &str,
        chain_code: &str,
        token_address: Option<String>,
        price_real: f64,
    ) -> Result<(), ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::PriceUpdate {
            symbol: symbol.to_string(),
            chain_code: chain_code.to_string(),
            token_address,
            price_real,
            response_tx,
        };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：获取钱包余额
    pub async fn get_wallet_balance(&self) -> Result<HashMap<String, BalanceInfo>, ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::GetWalletBalance { response_tx };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：获取账户余额
    pub async fn get_account_balance(
        &self,
        wallet_address: &str,
        chain_code: Option<String>,
    ) -> Result<HashMap<String, BalanceInfo>, ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::GetAccountBalance {
            wallet_address: wallet_address.to_string(),
            chain_code,
            response_tx,
        };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：获取余额摘要
    pub async fn get_balance_summary(
        &self,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
        chain_code: Option<&str>,
    ) -> Result<BalanceInfo, ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::GetBalanceSummary {
            wallet_address: wallet_address.map(|s| s.to_string()),
            account_id,
            chain_code: chain_code.map(|s| s.to_string()),
            response_tx,
        };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：初始化账户缓存
    pub async fn init_account_cache(&self) -> Result<(), ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::InitAccountCache { response_tx };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }

    // 异步接口：添加账户到缓存
    pub async fn add_account_to_cache(&self, address: &str, account_id: u32, wallet: &str) {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::AddAccountToCache {
            address: address.to_string(),
            account_id,
            wallet: wallet.to_string(),
            response_tx,
        };

        if let Err(e) = self.sender.send(msg).await {
            error!("Failed to send message: {}", e);
        }

        // 忽略响应
        let _ = response_rx.recv().await;
    }

    // 异步接口：获取总价值
    pub async fn get_total_usdt(&self) -> Decimal {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::GetTotalUsdt { response_tx };

        if let Err(e) = self.sender.send(msg).await {
            error!("Failed to send message: {}", e);
            return Decimal::ZERO;
        }

        response_rx.recv().await.unwrap_or(Decimal::ZERO)
    }

    // 异步接口：启动批处理任务
    pub async fn start_batch_recalculator(&self, interval_ms: u64) -> Result<(), ServiceError> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        let msg = AssetCalcMessage::StartBatchRecalculator { interval_ms, response_tx };

        self.sender.send(msg).await.map_err(|e| {
            ServiceError::System(SystemError::Service(format!("Failed to send message: {}", e)))
        })?;

        response_rx.recv().await.unwrap_or(Err(ServiceError::System(SystemError::Service(
            "No response received".to_string(),
        ))))
    }
}
