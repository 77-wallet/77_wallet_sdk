use crate::{
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
        },
        chain::{TransferResp, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    request::{
        api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
        transaction::BaseTransferReq,
    },
    service::transaction::TransactionService,
};
use wallet_database::{
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType, assets::AssetsId},
    repositories::{
        api_assets::ApiAssetsRepo, api_collect::ApiCollectRepo, api_wallet::ApiWalletRepo,
    },
};
use wallet_utils::conversion;

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect(req: &ApiWithdrawReq) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid, Some(ApiWalletType::SubAccount))
            .await?
            .ok_or(crate::BusinessError::ApiWallet(crate::ApiWalletError::NotFound))?;

        ApiCollectRepo::upsert_api_collect(
            &pool,
            &req.uid,
            &wallet.name,
            &req.from,
            &req.to,
            &req.value,
            &req.chain_code,
            req.token_address.clone(),
            &req.symbol,
            &req.trade_no,
            req.trade_type,
        )
        .await?;

        let backend_api = crate::Context::get_global_backend_api()?;
        // // 查询策略
        let strategy = backend_api.query_collection_strategy(&req.uid).await?;
        let threshold = strategy.threshold;

        let Some(chain_config) =
            strategy.chain_configs.iter().find(|config| config.chain_code == req.chain_code)
        else {
            return Err(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::ChainConfigNotFound(req.chain_code.to_owned()),
            )
            .into());
        };

        // 查询手续费
        let mut params = BaseTransferReq::new(
            &req.from,
            &req.to.to_string(),
            &req.value.to_string(),
            &req.chain_code.to_string(),
            &req.symbol.to_string(),
        );
        params.with_token(req.token_address.clone());

        let fee = TransactionService::transaction_fee(params).await?;

        // 查询资产余额
        let main_coin = ChainTransDomain::main_coin(&req.chain_code).await?;

        let main_symbol = main_coin.symbol;
        let assets_id = AssetsId::new(&req.from, &req.chain_code, &main_symbol, None);
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        // 如果手续费不足，则从其他地址转入手续费费用
        if conversion::decimal_from_str(&assets.balance)?
            < conversion::decimal_from_str(&fee.content)?
        {
            let withdraw_address = &chain_config.normal_address.address;
            // 查询出款地址余额主币余额
            let assets_id = AssetsId::new(withdraw_address, &req.chain_code, &main_symbol, None);

            let withdraw_assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
                .await?
                .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;
            let withdraw_balance = withdraw_assets.balance;
            // 出款地址余额够不够
            // if 不够 -> 出款地址余额 -> 不够 -> 通知后端：失败原因
            if conversion::decimal_from_str(&withdraw_balance)?
                < conversion::decimal_from_str(&fee.content)?
            {
                ApiCollectRepo::update_api_collect_status(
                    &pool,
                    &req.trade_no,
                    ApiCollectStatus::WithdrawInsufficientBalance,
                )
                .await?;
            }
            // else 够 -> 转账手续费 -> 通知后端手续费转账
            else {
                // 获取密码缓存
                let password = "";
                // 从出款地址转手续费到from_addr
                let coin = CoinDomain::get_coin(&req.chain_code, &main_symbol, None).await?;
                let mut params = ApiBaseTransferReq::new(
                    &req.from,
                    &req.to.to_string(),
                    &req.value.to_string(),
                    &req.chain_code.to_string(),
                );
                params.with_token(None, coin.decimals, &coin.symbol);

                let transfer_req = ApiTransferReq { base: params, password: password.to_string() };
                // 上链
                // 发交易
                let tx_resp = Self::transfer(transfer_req).await?;
                // ApiChainTransDomain::transfer(params, bill_kind, adapter)
                let resource_consume = if tx_resp.consumer.is_none() {
                    "0".to_string()
                } else {
                    tx_resp.consumer.unwrap().energy_used.to_string()
                };
            }
        }
        // 执行归集
        else {
            let coin =
                CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_address.clone())
                    .await?;
            let mut params = ApiBaseTransferReq::new(
                &req.from,
                &req.to.to_string(),
                &req.value.to_string(),
                &req.chain_code.to_string(),
            );
            params.with_token(coin.token_address, coin.decimals, &coin.symbol);

            let transfer_req = ApiTransferReq { base: params, password: "q1111111".to_string() };
            // 上链
            // 发交易
            let tx_resp = Self::transfer(transfer_req).await?;
            let resource_consume = if tx_resp.consumer.is_none() {
                "0".to_string()
            } else {
                tx_resp.consumer.unwrap().energy_used.to_string()
            };
            ApiCollectRepo::update_api_collect_tx_status(
                &pool,
                &req.trade_no,
                &tx_resp.tx_hash,
                &resource_consume,
                &tx_resp.fee,
                ApiCollectStatus::SendingTx,
            )
            .await?;
        }

        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }

    /// transfer
    pub async fn transfer(params: ApiTransferReq) -> Result<TransferResp, crate::ServiceError> {
        tracing::info!("transfer ------------------- 7:");
        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        tracing::info!("transfer ------------------- 8:");

        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(params.base.chain_code.as_str())
            .await?;

        let resp = adapter.transfer(&params, private_key).await?;

        tracing::info!("transfer ------------------- 10:");

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::manager::Context::get_global_backend_api()?;
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp)
    }
}
