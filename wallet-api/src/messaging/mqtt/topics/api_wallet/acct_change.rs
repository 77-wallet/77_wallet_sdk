use wallet_database::entities::bill::{BillExtraSwap, BillKind};

use crate::{
    infrastructure::inner_event::{InnerEvent, SyncAssetsData},
    messaging::{
        mqtt::topics::AcctChange,
        notify::{FrontendNotifyEvent, event::NotifyEvent, transaction::AcctChangeFrontend},
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
        if !acct_change.0.status {
            tracing::warn!("acct_change status is false, skip sync assets");
            return Ok(());
        }
        let handles = crate::context::CONTEXT.get().unwrap().get_global_handles();
        if let Some(handles) = handles.upgrade() {
            let inner_event_handle = handles.get_global_inner_event_handle();

            let data = SyncAssetsData::new(
                vec![acct_change.0.from_addr.clone(), acct_change.0.to_addr.clone()],
                acct_change.0.chain_code.clone(),
                acct_change.get_sync_assets_symbol(),
                acct_change.0.token.clone(),
            );
            inner_event_handle.send(InnerEvent::ApiWalletSyncAssets(data))?;

            crate::infrastructure::asset_calc::on_asset_update(
                &acct_change.0.from_addr,
                &acct_change.0.chain_code,
                &acct_change.0.token.clone().unwrap_or_default(),
            );
            crate::infrastructure::asset_calc::on_asset_update(
                &acct_change.0.to_addr,
                &acct_change.0.chain_code,
                &acct_change.0.token.clone().unwrap_or_default(),
            );
        } else {
            tracing::warn!("acct_change status is false, skip sync assets");
        }
        // tracing::info!("发送同步资产事件");
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
