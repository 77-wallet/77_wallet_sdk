use crate::{
    domain::{
        api_wallet::{
            account::ApiAccountDomain,
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
            fee::ApiFeeDomain,
        },
        chain::{TransferResp, transaction::ChainTransDomain},
        coin::CoinDomain,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
};
use wallet_database::{
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
    repositories::{api_collect::ApiCollectRepo, api_wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::transaction::FeeCollectionUploadReq;
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};

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

        // 查询手续费
        let main_coin = ChainTransDomain::main_coin(&req.chain_code).await?;
        tracing::info!("主币： {}", main_coin.symbol);
        let main_symbol = main_coin.symbol;
        let fee = Self::estimate_fee(
            &req.from,
            &req.to,
            &req.value,
            &req.chain_code,
            &req.symbol,
            &main_symbol,
            req.token_address.clone(),
            main_coin.decimals,
        )
        .await?;
        tracing::info!("估算手续费: {}", fee);

        let backend_api = crate::Context::get_global_backend_api()?;
        // 查询策略
        let strategy = backend_api.query_collection_strategy(&req.uid).await?;
        let Some(chain_config) =
            strategy.chain_configs.iter().find(|config| config.chain_code == req.chain_code)
        else {
            return Err(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::ChainConfigNotFound(req.chain_code.to_owned()),
            )
            .into());
        };

        // 查询资产主币余额
        let balance = Self::query_balance(&req.from, &req.chain_code, None).await?;

        tracing::info!("from: {}, to: {}", req.from, req.to);
        tracing::info!("资产主币余额: {balance}, 手续费: {fee}");

        let balance = conversion::decimal_from_str(&balance)?;
        let fee_decimal = conversion::decimal_from_str(&fee.to_string())?;
        let need = if req.token_address.is_some() { fee_decimal } else { fee_decimal + balance };
        tracing::info!("need: {need}");
        // 如果手续费不足，则从其他地址转入手续费费用
        if balance < need {
            // 查询出款地址余额主币余额
            // let withdraw_address = &chain_config.normal_address.address;
            // let withdraw_address = "TBQSs8KG82iQnLUZj5nygJzSUwwhQJcxHF";
            let withdraw_address = "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt";

            let withdraw_balance =
                Self::query_balance(withdraw_address, &req.chain_code, None).await?;

            tracing::info!(
                "subaccount wallet balance not enough，subaccount balance: {withdraw_balance}"
            );
            // todo!();
            // let assets_id = AssetsId::new(withdraw_address, &req.chain_code, &main_symbol, None);

            // let withdraw_assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            //     .await?
            //     .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;
            // let withdraw_balance = withdraw_assets.balance;
            // 出款地址余额够不够
            // if 不够 -> 出款地址余额 -> 不够 -> 通知后端：失败原因
            let withdraw_balance = conversion::decimal_from_str(&withdraw_balance)?;
            if withdraw_balance < fee_decimal {
                ApiCollectRepo::update_api_collect_status(
                    &pool,
                    &req.trade_no,
                    ApiCollectStatus::WithdrawInsufficientBalance,
                )
                .await?;
                tracing::info!("withdraw wallet balance not enough");
            }
            // else 够 -> 转账手续费 -> 通知后端手续费转账
            else {
                let coin = CoinDomain::get_coin(&req.chain_code, &main_symbol, None).await?;
                let fee = unit::format_to_string(fee, coin.decimals)?;

                let backend_api = crate::Context::get_global_backend_api()?;
                let req = FeeCollectionUploadReq::new(
                    &req.trade_no,
                    &req.chain_code,
                    &main_symbol,
                    "",
                    &withdraw_address,
                    &req.from,
                    unit::string_to_f64(&fee)?,
                );

                backend_api.upload_fee_collection_record(&req).await?;
            }
        }
        // 执行归集
        else {
            tracing::info!("执行归集");
            let coin =
                CoinDomain::get_coin(&req.chain_code, &req.symbol, req.token_address.clone())
                    .await?;
            let mut params = ApiBaseTransferReq::new(
                &req.from,
                &req.to.to_string(),
                &req.value.to_string(),
                &req.chain_code.to_string(),
            );
            params.with_token(coin.token_address(), coin.decimals, &coin.symbol);

            let transfer_req = ApiTransferReq { base: params, password: "q1111111".to_string() };
            // 上链
            // 发交易
            let tx_resp = ApiFeeDomain::transfer(transfer_req).await?;
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

    async fn query_balance(
        owner_address: &str,
        chain_code: &str,
        token_address: Option<String>,
    ) -> Result<String, crate::ServiceError> {
        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(chain_code)
            .await?;
        let account = adapter.balance(&owner_address, token_address).await?;
        Ok(account.to_string())
    }

    async fn estimate_fee(
        from: &str,
        to: &str,
        value: &str,
        chain_code: &str,
        symbol: &str,
        main_symbol: &str,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<alloy::primitives::U256, crate::ServiceError> {
        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(chain_code)
            .await?;

        let mut params = ApiBaseTransferReq::new(from, to, value, chain_code);
        params.with_token(token_address, decimals, symbol);
        let fee = adapter.estimate_fee(params, main_symbol).await?;

        let chain_code: ChainCode = chain_code.try_into()?;
        let amount = match chain_code {
            ChainCode::Tron => fee,
            ChainCode::Bitcoin => todo!(),
            ChainCode::Solana => todo!(),
            ChainCode::Ethereum => todo!(),
            ChainCode::BnbSmartChain => todo!(),
            ChainCode::Litecoin => todo!(),
            ChainCode::Dogcoin => todo!(),
            ChainCode::Sui => todo!(),
            ChainCode::Ton => todo!(),
        };
        let amount = unit::convert_to_u256(&amount, decimals)?;
        Ok(amount)
    }
}
