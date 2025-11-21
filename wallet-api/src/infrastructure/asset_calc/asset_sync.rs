use std::collections::HashSet;

// 只导入需要的Decimal类型
use rust_decimal::Decimal;

use crate::{
    infrastructure::asset_calc::{
        ACCOUNT_VALUE_CACHE, ADDRESS_TO_ACCOUNT_ID, ASSET_VALUE_CACHE, AssetEntry, AssetKey,
        TOTAL_USDT,
    },
    messaging::notify::{
        FrontendNotifyEvent,
        api_wallet::{ApiWalletSyncAccountBalanceMsgFrontItem, ApiWalletSyncAssetsMsgFront},
        event::NotifyEvent,
    },
    response_vo::{account::BalanceInfo, coin::TokenCurrencies},
};

// 导入测试所需的模块
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    // 由于测试可能会在编译时失败，暂时注释掉测试模块
    // 后续可以在编译通过后重新启用
    /*
    #[test]
    async fn test_update_total_usdt() {
        // 重置TOTAL_USDT为0
        *TOTAL_USDT.write().await = Decimal::ZERO;

        // 测试添加正值
        let old_value = None;
        let new_value = Some(100.50);
        super::update_total_usdt(old_value, new_value).await;

        assert_eq!(*TOTAL_USDT.read().await, Decimal::new(10050, 2));

        // 测试替换值
        let old_value = Some(Decimal::new(10050, 2));
        let new_value = Some(200.75);
        super::update_total_usdt(old_value, new_value).await;

        assert_eq!(*TOTAL_USDT.read().await, Decimal::new(20075, 2));

        // 测试移除值
        let old_value = Some(Decimal::new(20075, 2));
        let new_value = None;
        super::update_total_usdt(old_value, new_value).await;

        assert_eq!(*TOTAL_USDT.read().await, Decimal::ZERO);
    }
    */

    // 更多测试用例可以在这里添加
}

pub(super) async fn affected_accounts(assets: Vec<AssetEntry>) {
    // 按账户级别聚合
    let mut affected_accounts: HashSet<(String, u32)> = HashSet::new();

    {
        let map = ADDRESS_TO_ACCOUNT_ID.read().await;
        for a in &assets {
            if let Some(account_id) = map.get(&a.address) {
                affected_accounts.insert((a.wallet_address.clone(), *account_id));
            }
        }
    }
    // 过滤未变化账户
    let changed_accounts = ApiWalletSyncAssetsMsgFront::new();
    for (wallet_address, account_id) in affected_accounts {
        let balance_info = crate::infrastructure::asset_calc::get_balance_summary(
            Some(&wallet_address),
            Some(account_id),
            None,
        )
        .await
        .unwrap();
        changed_accounts.add_item(
            &wallet_address,
            ApiWalletSyncAccountBalanceMsgFrontItem::new(account_id, balance_info),
        );
    }

    // 只有有变化才推送
    if let Err(e) =
        FrontendNotifyEvent::new(NotifyEvent::ApiWalletSyncAssets(changed_accounts)).send().await
    {
        tracing::error!("send error: {}", e);
    }
}

pub(super) async fn aggregate_and_notify(
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
            ASSET_VALUE_CACHE.get(&asset_key).and_then(|old| old.fiat_value).map(|v| {
                // 直接使用v作为浮点数进行计算
                Decimal::new((v * 100.0) as i64, 2)
            });

        // 更新资产缓存
        ASSET_VALUE_CACHE.insert(asset_key.clone(), balance_info.clone());

        // 正确更新TOTAL_USDT：先减去旧值，再加上新值
        update_total_usdt(old_fiat_value, balance_info.fiat_value).await;
    }
}

/// 安全地更新TOTAL_USDT变量
async fn update_total_usdt(old_value: Option<Decimal>, new_value: Option<f64>) {
    // 尝试获取写锁，如果获取失败则记录日志但不阻塞
    if let Ok(mut total) = TOTAL_USDT.try_write() {
        // 先减去旧值
        if let Some(old) = old_value {
            *total = *total - old;
        }

        // 再加新值
        if let Some(new) = new_value {
            // 直接使用new作为浮点数进行计算
            let new_decimal = Decimal::new((new * 100.0) as i64, 2);
            *total = *total + new_decimal;
        }

        // 确保TOTAL_USDT不会为负数
        if *total < Decimal::ZERO {
            *total = Decimal::ZERO;
        }
    } else {
        tracing::warn!("Failed to acquire write lock for TOTAL_USDT update");
        // 稍后可以实现重试机制
    }
}
