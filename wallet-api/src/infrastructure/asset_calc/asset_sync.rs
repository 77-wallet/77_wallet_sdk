use std::collections::{HashMap, HashSet};

use crate::{
    infrastructure::asset_calc::{
        ACCOUNT_VALUE_CACHE, ADDRESS_TO_ACCOUNT_ID, ASSET_VALUE_CACHE, AssetEntry, AssetKey,
    },
    messaging::notify::{
        FrontendNotifyEvent,
        api_wallet::{ApiWalletSyncAccountBalanceMsgFrontItem, ApiWalletSyncAssetsMsgFront},
        event::NotifyEvent,
    },
    response_vo::{account::BalanceInfo, coin::TokenCurrencies},
};

pub(super) async fn aggregate_and_notify(
    assets: Vec<AssetEntry>,
    token_currencies_snapshot: TokenCurrencies,
    currency: String,
) {
    use rayon::prelude::*;

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

    assets.par_iter().for_each(|a| {
        let asset_key =
            AssetKey::new(&a.wallet_address, &a.address, &a.chain_code, &a.token_address);

        let balance_info = token_currencies_snapshot
            .calculate_sync_to_balance(
                &currency,
                &a.balance.to_string(),
                &a.symbol,
                &a.chain_code,
                Some(a.token_address.clone()),
            )
            .unwrap_or(BalanceInfo {
                amount: 0.0,
                currency: currency.clone(),
                unit_price: None,
                fiat_value: Some(0.0),
            });

        // 更新资产缓存
        ASSET_VALUE_CACHE.insert(asset_key, balance_info.clone());
    });

    // 过滤未变化账户
    let mut changed_accounts = ApiWalletSyncAssetsMsgFront::new();
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
