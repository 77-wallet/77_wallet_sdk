use crate::{
    domain::{
        api_wallet::{
            adapter_factory::{API_ADAPTER_FACTORY, ApiChainAdapterFactory},
            trans::ApiTransDomain,
            wallet::ApiWalletDomain,
        },
        chain::transaction::ChainTransDomain,
        coin::CoinDomain,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
};
use wallet_database::{
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
    repositories::{api_collect::ApiCollectRepo, api_wallet::ApiWalletRepo},
};
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig, transaction::ServiceFeeUploadReq,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;

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
            ApiCollectStatus::Init,
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

        // 查询策略
        let chain_config = Self::get_collect_config(&req.uid, &req.chain_code).await?;

        // 查询资产主币余额
        let balance =
            Self::query_balance(&req.from, &req.chain_code, None, main_coin.decimals).await?;

        tracing::info!("from: {}, to: {}", req.from, req.to);
        tracing::info!("资产主币余额: {balance}, 手续费: {fee}");

        let balance = conversion::decimal_from_str(&balance)?;
        let value = conversion::decimal_from_str(&req.value)?;
        let fee_decimal = conversion::decimal_from_str(&fee.to_string())?;
        let need = if req.token_address.is_some() { fee_decimal } else { fee_decimal + value };
        tracing::info!("need: {need}");
        // 如果手续费不足，则从其他地址转入手续费费用
        if balance < need {
            // 查询出款地址余额主币余额
            let withdraw_address = &chain_config.normal_address.address;
            // let withdraw_address = "TBQSs8KG82iQnLUZj5nygJzSUwwhQJcxHF";
            // let withdraw_address = "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt";

            let withdraw_balance =
                Self::query_balance(withdraw_address, &req.chain_code, None, main_coin.decimals)
                    .await?;

            tracing::info!(
                "subaccount wallet balance not enough，subaccount balance: {withdraw_balance}"
            );
            // 出款地址余额够不够
            // if 不够 -> 出款地址余额 -> 不够 -> 通知后端：失败原因
            let withdraw_balance = conversion::decimal_from_str(&withdraw_balance)?;
            if withdraw_balance < fee_decimal {
                ApiCollectRepo::update_api_collect_status(
                    &pool,
                    &req.trade_no,
                    ApiCollectStatus::InsufficientBalance,
                    "insufficient balance"
                )
                .await?;

                tracing::info!("withdraw wallet balance not enough");
                // todo!();
            }
            // else 够 -> 转账手续费 -> 通知后端手续费转账
            else {
                // let coin = CoinDomain::get_coin(&req.chain_code, &main_symbol, None).await?;
                // let fee = unit::format_to_string(fee, coin.decimals)?;
                tracing::info!("need transfer withdraw fee");
                let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
                let req = ServiceFeeUploadReq::new(
                    &req.trade_no,
                    &req.chain_code,
                    &main_symbol,
                    "",
                    &withdraw_address,
                    &req.from,
                    unit::string_to_f64(&fee)?,
                );

                backend_api.upload_service_fee_record(&req).await?;
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

            let password = ApiWalletDomain::get_passwd().await?;

            // todo!();

            let transfer_req = ApiTransferReq { base: params, password };
            // 上链
            // 发交易
            let tx_resp = ApiTransDomain::transfer(transfer_req).await?;
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

        Ok(())
    }

    pub(crate) async fn collect_v2(
        req: &ApiWithdrawReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;

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

        // 查询策略
        let chain_config = Self::get_collect_config(&req.uid, &req.chain_code).await?;

        // 查询资产主币余额
        let balance =
            Self::query_balance(&req.from, &req.chain_code, None, main_coin.decimals).await?;

        tracing::info!("from: {}, to: {}", req.from, req.to);
        tracing::info!("资产主币余额: {balance}, 手续费: {fee}");

        let balance = conversion::decimal_from_str(&balance)?;
        let value = conversion::decimal_from_str(&req.value)?;
        let fee_decimal = conversion::decimal_from_str(&fee.to_string())?;
        let need = if req.token_address.is_some() { fee_decimal } else { fee_decimal + value };
        tracing::info!("need: {need}");
        // 如果手续费不足，则从其他地址转入手续费费用
        if balance < need {
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
                ApiCollectStatus::InsufficientBalance,
            ).await?;

            ApiCollectRepo::update_api_collect_status(
                &pool,
                &req.trade_no,
                ApiCollectStatus::InsufficientBalance,
                "insufficient balance",
            ).await?;
            tracing::info!("need transfer withdraw fee");
            let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let req = ServiceFeeUploadReq::new(
                &req.trade_no,
                &req.chain_code,
                &main_symbol,
                "",
                &req.to,
                &req.from,
                unit::string_to_f64(&fee)?,
            );

            backend_api.upload_service_fee_record(&req).await?;
        } else {
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
                ApiCollectStatus::SufficientBalance,
            ).await?;

            // 可能发交易
            if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
            {
                handles.get_global_processed_collect_tx_handle().submit_tx(&req.trade_no).await?;
            }
        }
        Ok(())
    }

    async fn get_collect_config(
        uid: &str,
        chain_code: &str,
    ) -> Result<ChainConfig, crate::error::service::ServiceError> {
        // 查询策略
        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let strategy = backend_api.query_collect_strategy(uid).await?;
        let Some(chain_config) =
            strategy.chain_configs.into_iter().find(|config| config.chain_code == chain_code)
        else {
            return Err(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::ChainConfigNotFound(
                    chain_code.to_owned(),
                ),
            )
            .into());
        };

        Ok(chain_config)
    }

    async fn query_balance(
        owner_address: &str,
        chain_code: &str,
        token_address: Option<String>,
        decimals: u8,
    ) -> Result<String, crate::error::service::ServiceError> {
        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(chain_code)
            .await?;
        let account = adapter.balance(&owner_address, token_address).await?;
        let ammount = unit::format_to_string(account, decimals)?;
        Ok(ammount)
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
    ) -> Result<String, crate::error::service::ServiceError> {
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
        // let amount = unit::convert_to_u256(&amount, decimals)?;
        Ok(amount)
    }

    pub async fn confirm_tx(
        trade_no: &str,
        status: ApiCollectStatus,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiCollectRepo::update_api_collect_status(&pool, trade_no, status, "confirm").await?;

        if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
        {
            handles
                .get_global_processed_collect_tx_handle()
                .submit_confirm_report_tx(trade_no)
                .await?;
        }
        Ok(())
    }
}
