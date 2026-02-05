use crate::{
    domain::chain::adapter::ChainAdapterFactory,
    error::{business::api_wallet::ApiWalletError, service::ServiceError},
    infrastructure::inner_event::{InnerEvent, SyncAssetsData},
    messaging::{
        mqtt::topics::AcctChange,
        notify::{FrontendNotifyEvent, event::NotifyEvent, transaction::AcctChangeFrontend},
    },
};
use chrono::{DateTime, NaiveDateTime, Utc};
use wallet_database::{
    entities::{
        api_assets::ApiCreateAssetsVo,
        api_coin::ApiCoinData,
        api_trade_type::ApiTradeType,
        api_wallet::ApiWalletType,
        api_withdraw::ApiWithdrawStatus,
        assets::{AssetsId, AssetsIdVo},
        bill::{BillExtraSwap, BillKind},
    },
    repositories::api_wallet::{
        account::ApiAccountRepo, assets::ApiAssetsRepo, coin::ApiCoinRepo, wallet::ApiWalletRepo,
        withdraw::ApiWithdrawRepo,
    },
};

// biz_type = ACCT_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiWalletAcctChange(AcctChange);

impl From<&ApiWalletAcctChange> for AcctChangeFrontend {
    fn from(value: &ApiWalletAcctChange) -> Self {
        Self {
            tx_hash: value.0.tx_hash.clone(),
            chain_code: value.0.chain_code.clone(),
            symbol: value.0.symbol.clone(),
            transfer_type: value.0.transfer_type,
            tx_kind: value.0.tx_kind,
            from_addr: value.0.from_addr.clone(),
            to_addr: value.0.to_addr.clone(),
            token: value.0.token.clone(),
            value: value.0.value,
            transaction_fee: value.0.transaction_fee,
            transaction_time: value.0.transaction_time.clone(),
            status: value.0.status,
            is_multisig: value.0.is_multisig,
            queue_id: value.0.queue_id.clone(),
            block_height: value.0.block_height,
            notes: value.0.notes.clone(),
        }
    }
}

impl ApiWalletAcctChange {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        // let event_name = self.name();
        tracing::debug!("处理帐变: {:?}", self);
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;

        if let Some(token_str) = &self.0.token {
            let has_coin = ApiCoinRepo::has_coin(&self.0.chain_code, token_str, &pool).await?;
            if !has_coin {
                if let Err(e) =
                    Self::try_create_coin_for_address(&self.0.chain_code, token_str).await
                {
                    tracing::error!("3deposit_acct_change 自动创建代币失败: to_addr {}", e);
                }
            }
        }

        // 充值帐变消息
        self.deposit_acct_change().await?;

        // 自己转账帐变
        self.self_transfer_acct_change().await?;

        // 更新资产,不进行新增(垃圾币)
        Self::sync_assets(&self).await?;

        // send acct_change to frontend
        let change_frontend = AcctChangeFrontend::from(self);
        let data = NotifyEvent::ApiWalletAcctChange(change_frontend);
        FrontendNotifyEvent::new(data).send().await?;
        Ok(())
    }

    async fn sync_assets(
        acct_change: &ApiWalletAcctChange,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;

        // 记录帐变信息用于调试
        tracing::info!(
            "开始同步资产: tx_hash={}, chain_code={}, symbol={}, from_addr={}, to_addr={}, status={}, token={:?}",
            acct_change.0.tx_hash,
            acct_change.0.chain_code,
            acct_change.0.symbol,
            acct_change.0.from_addr,
            acct_change.0.to_addr,
            acct_change.0.status,
            acct_change.0.token
        );

        // 优化：即使 status=false，也尝试同步（可能是失败交易但余额已变化）
        if !acct_change.0.status {
            tracing::warn!(
                "帐变状态为失败，但仍尝试同步资产: tx_hash={}, chain_code={}",
                acct_change.0.tx_hash,
                acct_change.0.chain_code
            );
        }

        // 尝试获取 coin 信息（用于创建资产记录），但不强制要求
        let coin = ApiCoinRepo::get_coin_by_chain_code_token_address(
            &pool,
            &acct_change.0.chain_code,
            &acct_change.0.token.clone().unwrap_or_default(),
        )
        .await?;

        // 如果 coin 不存在，尝试自动创建
        let coin = if (coin.is_none() && acct_change.0.token.is_some())
            || (coin.is_some()
                && coin.clone().unwrap().price.parse::<f64>().is_ok()
                && coin.clone().unwrap().price.parse::<f64>().unwrap() == 0.0f64)
        {
            tracing::info!(
                "coin 信息 有误，尝试自动创建代币或者更新: chain_code={}, token={:?}。{coin:?}",
                acct_change.0.chain_code,
                acct_change.0.token
            );

            // 重新查询 coin
            ApiCoinRepo::get_coin_by_chain_code_token_address(
                &pool,
                &acct_change.0.chain_code,
                &acct_change.0.token.clone().unwrap_or_default(),
            )
            .await?
        } else {
            coin
        };

        if coin.is_none() {
            tracing::warn!(
                "未找到 coin 信息，将跳过资产记录创建，但仍尝试同步已存在的资产: chain_code={}, token={:?}",
                acct_change.0.chain_code,
                acct_change.0.token
            );
        }

        let addrs = vec![acct_change.0.from_addr.clone(), acct_change.0.to_addr.clone()];
        let mut sync_addrs = Vec::new();

        // 优化：即使找不到 account，如果数据库中有资产记录，也应该同步
        for addr in addrs.iter() {
            let account = ApiAccountRepo::find_one_by_address_chain_code(
                addr,
                &acct_change.0.chain_code,
                &pool,
            )
            .await?;

            // 如果找到 account，尝试创建资产记录（如果不存在）
            if let Some(account) = &account {
                if let Some(ref coin) = coin {
                    let assets_id_vo = AssetsIdVo::new(
                        addr,
                        &acct_change.0.chain_code,
                        acct_change.0.token.clone(),
                    );
                    let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id_vo).await?;
                    if assets.is_none() {
                        let assets_id = AssetsId::new(
                            &account.address,
                            &account.chain_code,
                            &coin.symbol,
                            coin.token_address.clone(),
                        );
                        let assets = ApiCreateAssetsVo::new(
                            assets_id,
                            coin.decimals,
                            coin.protocol.clone(),
                            0,
                        )
                        .with_name(&coin.name)
                        .with_u256(alloy::primitives::U256::default(), coin.decimals)?;
                        ApiAssetsRepo::upsert_assets_multi(&pool, vec![assets]).await?;
                        tracing::info!(
                            "创建资产记录: address={}, chain_code={}, symbol={}",
                            account.address,
                            account.chain_code,
                            coin.symbol
                        );
                    }
                }
            }

            // 优化：即使找不到 account，如果数据库中有该地址的资产记录，也应该同步
            let assets_id_vo =
                AssetsIdVo::new(addr, &acct_change.0.chain_code, acct_change.0.token.clone());
            let existing_assets = ApiAssetsRepo::find_by_id(&pool, &assets_id_vo).await?;

            if account.is_some() || existing_assets.is_some() {
                sync_addrs.push(addr.to_string());
            } else {
                tracing::debug!(
                    "跳过地址（无 account 且无资产记录）: address={}, chain_code={}",
                    addr,
                    acct_change.0.chain_code
                );
            }
        }

        if sync_addrs.is_empty() {
            tracing::warn!(
                "没有需要同步的地址: tx_hash={}, chain_code={}",
                acct_change.0.tx_hash,
                acct_change.0.chain_code
            );
            return Ok(());
        }

        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles().await;
        if let Some(handles) = handles.upgrade() {
            let inner_event_handle = handles.get_global_inner_event_handle();

            let symbols = acct_change.get_sync_assets_symbol();
            let data = SyncAssetsData::new(
                sync_addrs.clone(),
                acct_change.0.chain_code.clone(),
                symbols.clone(),
                acct_change.0.token.clone(),
            );

            tracing::info!(
                "发送资产同步事件: tx_hash={}, addrs={:?}, chain_code={}, symbols={:?}, token={:?}",
                acct_change.0.tx_hash,
                sync_addrs,
                acct_change.0.chain_code,
                symbols,
                acct_change.0.token
            );

            inner_event_handle.send(InnerEvent::ApiWalletSyncAssets(data))?;
        } else {
            tracing::error!(
                "Handles 已释放，无法发送资产同步事件: tx_hash={}",
                acct_change.0.tx_hash
            );
        }

        Ok(())
    }

    // 尝试为地址创建代币
    async fn try_create_coin_for_address(
        chain_code: &str,
        token_address: &str,
    ) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        tracing::error!("为地址创建代币22: chain_code={}, token={}", chain_code, token_address);
        if token_address.is_empty() {
            return Ok(());
        }

        let chain_instance = ChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let coins_finds = backend_api.fetch_all_api_tokens(None, None).await?;
        tracing::error!(
            "1try_create_coin_for_address find token coin , price is :{:?}",
            coins_finds
        );

        let coin_find = coins_finds.iter().find(|o| {
            o.token_address == Some(token_address.to_string())
                && o.chain_code == Some(chain_code.to_string())
        });

        tracing::error!(
            "2try_create_coin_for_address Create new token coin , price is :{:?}",
            coin_find
        );
        let time = wallet_utils::time::now();
        let symbol = chain_instance.token_symbol(&token_address).await?;
        let name = chain_instance.token_name(&token_address).await?;
        let cus_coin = ApiCoinData::new(
            Some(name.clone()),
            &symbol,
            chain_code,
            Some(token_address.to_string()),
            coin_find.map(|x| x.price.map(|o| o.to_string())).unwrap_or_default(),
            None,
            chain_instance.decimals(&token_address).await?,
            1,
            0,
            1,
            time,
            Some(time),
        )
        .with_custom(0)
        .with_status(1);
        let coin = vec![cus_coin];
        tracing::error!("[55customize_coin] coin: {:?} ", coin);
        ApiCoinRepo::upsert_multi_coin(&pool, coin).await?;
        tracing::error!("成功创建代币: chain_code={}, token={}", chain_code, token_address);

        Ok(())
    }

    // 需要更新的资产-swap 需要更新swap的资产
    fn get_sync_assets_symbol(&self) -> Vec<String> {
        let mut symbol = vec![self.0.symbol.clone()];
        // 由于目前swap会发送躲多币交易,z这个地方取消
        if self.0.tx_kind == BillKind::Swap.to_i8() {
            if let Some(extra) = &self.0.extra {
                if let Ok(extra_swap) =
                    wallet_utils::serde_func::serde_from_value::<BillExtraSwap>(extra.clone())
                {
                    if self.0.symbol != extra_swap.from_token_symbol {
                        symbol.push(extra_swap.from_token_symbol);
                    }
                    symbol.push(extra_swap.to_token_symbol);
                }
            }
        }
        symbol
    }

    async fn deposit_acct_change(&self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let api_funds_pool = crate::get_context()?.api_funds_pool()?;
        let to_account = ApiAccountRepo::find_one_by_address_chain_code(
            &self.0.to_addr,
            &self.0.chain_code,
            &pool,
        )
        .await?;
        if let Some(to_account) = to_account {
            if to_account.api_wallet_type == ApiWalletType::Withdrawal {
                let from_account = ApiAccountRepo::find_one_by_address_chain_code(
                    &self.0.from_addr,
                    &self.0.chain_code,
                    &pool,
                )
                .await?;
                if let None = from_account {
                    let wallet =
                        ApiWalletRepo::find_by_address(&pool, &to_account.wallet_address).await?;
                    if let Some(wallet) = wallet {
                        let datetime =
                            self.convert_transaction_time(self.0.transaction_time.as_str())?;
                        let resource_consume = if let Some(energy_used) = self.0.energy_used {
                            energy_used.to_string()
                        } else {
                            "".to_string()
                        };
                        let trade_no = uuid::Uuid::new_v4().to_string();
                        ApiWithdrawRepo::upsert_api_withdraw(
                            &api_funds_pool,
                            &wallet.uid,
                            &wallet.name,
                            self.0.from_addr.as_str(),
                            self.0.to_addr.as_str(),
                            self.0.value.to_string().as_str(),
                            "",
                            &self.0.chain_code,
                            self.0.token.clone(),
                            self.0.symbol.as_str(),
                            &trade_no,
                            ApiTradeType::SelfRecharge,
                            0,
                            Some(self.0.tx_hash.clone()),
                            ApiWithdrawStatus::ConfirmSuccessReport,
                            ApiWithdrawStatus::ConfirmSuccessReport,
                            resource_consume.as_str(),
                            self.0.transaction_fee.to_string().as_str(),
                            Some(datetime),
                            Some(self.0.block_height.to_string()),
                        )
                        .await?;
                    }
                } else {
                    tracing::warn!(to_account=%to_account.address, "from account found:");
                }
            }
        }
        Ok(())
    }

    async fn self_transfer_acct_change(&self) -> Result<(), ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().core_pool()?;
        let api_funds_pool = crate::get_context()?.api_funds_pool()?;
        let from_account = ApiAccountRepo::find_one_by_address_chain_code(
            &self.0.from_addr,
            &self.0.chain_code,
            &pool,
        )
        .await?;
        if let Some(from_account) = from_account {
            if from_account.api_wallet_type == ApiWalletType::Withdrawal {
                let res = ApiWithdrawRepo::get_by_hash_and_owner(
                    &api_funds_pool,
                    self.0.from_addr.as_str(),
                    &self.0.tx_hash,
                )
                .await;
                match res {
                    Ok(tx) => {
                        if tx.trade_type == ApiTradeType::SelfWithdraw {
                            let status = if self.0.status {
                                ApiWithdrawStatus::ConfirmSuccessReport
                            } else {
                                ApiWithdrawStatus::ConfirmFailureReport
                            };
                            let datetime =
                                self.convert_transaction_time(self.0.transaction_time.as_str())?;
                            let resource_consume = if let Some(energy_used) = self.0.energy_used {
                                energy_used.to_string()
                            } else {
                                "0".to_string()
                            };
                            ApiWithdrawRepo::update_api_withdraw_tx_status(
                                &api_funds_pool,
                                &tx.trade_no,
                                0,
                                &tx.tx_hash.unwrap_or_default(),
                                &resource_consume,
                                self.0.transaction_fee.to_string().as_str(),
                                Some(datetime),
                                self.0.block_height.to_string().as_str(),
                                status,
                            )
                            .await?;
                        } else if tx.trade_type == ApiTradeType::Withdraw {
                            let datetime =
                                self.convert_transaction_time(self.0.transaction_time.as_str())?;
                            let resource_consume = if let Some(energy_used) = self.0.energy_used {
                                energy_used.to_string()
                            } else {
                                "0".to_string()
                            };
                            ApiWithdrawRepo::update_api_withdraw_tx(
                                &api_funds_pool,
                                &tx.trade_no,
                                &resource_consume,
                                self.0.transaction_fee.to_string().as_str(),
                                Some(datetime),
                                self.0.block_height.to_string().as_str(),
                            )
                            .await?;
                        } else {
                            tracing::warn!("api_wallet_type == {:?} is not found:", tx.trade_type);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("api_wallet_type == Withdrawal is not found: {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    fn convert_transaction_time(
        &self,
        transaction_time: &str,
    ) -> Result<DateTime<Utc>, ServiceError> {
        let naive =
            NaiveDateTime::parse_from_str(transaction_time, "%Y-%m-%d %H:%M:%S").map_err(|_| {
                ServiceError::Business(
                    ApiWalletError::DataTimeParseError(transaction_time.to_string()).into(),
                )
            })?;
        let datetime: DateTime<Utc> = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        Ok(datetime)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        messaging::mqtt::topics::api_wallet::acct_change::ApiWalletAcctChange,
        test::env::get_manager,
    };

    async fn init_manager() {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await.unwrap();
    }

    // 普通账交易
    #[tokio::test]
    async fn acct_change() -> anyhow::Result<()> {
        init_manager().await;

        let change = r#"{"txHash":"c357a09e84a6dd1ad0d621641320f505fd23bc3c48251a5d524fd281de2870da:ftIuBQWDNv8Ik9FQy8aUIfzdrTbennywxOCmw6Ury1A=","chainCode":"ton","symbol":"TON","transferType":0,"txKind":1,"fromAddr":"UQDaL1eH_9TU3hceiO7ZsPDEdcmwDhZ0eDZ_NCOIrmjHoSQb","toAddr":"UQAJr_aCqkWARCMkTHYkpKL9B-kYOFvXxvyDumUXsZ79ZnYY","token":"","value":0.01,"transactionFee":0.002432489,"transactionTime":"2025-06-17 08:53:28","status":true,"isMultisig":0,"queueId":"","blockHeight":48927711,"notes":"","netUsed":0,"energyUsed":null}"#;
        let change = serde_json::from_str::<ApiWalletAcctChange>(&change).unwrap();
        let _res = change.exec("2").await.unwrap();

        let change = r#"{"txHash":"c357a09e84a6dd1ad0d621641320f505fd23bc3c48251a5d524fd281de2870da:ftIuBQWDNv8Ik9FQy8aUIfzdrTbennywxOCmw6Ury1A=","chainCode":"ton","symbol":"TON","transferType":1,"txKind":1,"fromAddr":"UQDaL1eH_9TU3hceiO7ZsPDEdcmwDhZ0eDZ_NCOIrmjHoSQb","toAddr":"UQAJr_aCqkWARCMkTHYkpKL9B-kYOFvXxvyDumUXsZ79ZnYY","token":"","value":0.01,"transactionFee":0.002432489,"transactionTime":"2025-06-17 08:53:28","status":true,"isMultisig":0,"queueId":"","blockHeight":48927711,"notes":"","netUsed":0,"energyUsed":null}"#;
        let change = serde_json::from_str::<ApiWalletAcctChange>(&change).unwrap();

        let _res = change.exec("1").await.unwrap();
        Ok(())
    }
}
