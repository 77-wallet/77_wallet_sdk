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
    messaging::notify::{api_wallet::WithdrawFront, event::NotifyEvent, FrontendNotifyEvent},
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
};
use wallet_database::{
    entities::{api_collect::ApiCollectStatus, api_wallet::ApiWalletType},
    repositories::{
        api_wallet::collect::ApiCollectRepo, api_wallet::wallet::ApiWalletRepo, api_wallet::withdraw::ApiWithdrawRepo,
    },
};
use wallet_transport_backend::request::api_wallet::{
    strategy::ChainConfig,
    transaction::{ServiceFeeUploadReq, TransAckType, TransEventAckReq, TransType},
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{conversion, unit};
use crate::request::api_wallet::trans::ApiCollectReq;

pub struct ApiCollectDomain {}

impl ApiCollectDomain {
    pub(crate) async fn collect_v2(
        req: &ApiCollectReq,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(&pool, &req.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;

        let res = ApiCollectRepo::get_api_collect_by_trade_no(&pool, &req.trade_no).await;
        if res.is_err() {
            ApiCollectRepo::upsert_api_collect(
                &pool,
                &req.uid,
                &wallet.name,
                &req.from,
                &req.to,
                &req.value,
                &req.validate,
                &req.chain_code,
                req.token_address.clone(),
                &req.symbol,
                &req.trade_no,
                req.trade_type,
                ApiCollectStatus::Init,
            )
            .await?;

            tracing::info!("upsert_api_collect  ------------------- 5: ",);

            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let trans_event_req =
                TransEventAckReq::new(&req.trade_no, TransType::Col, TransAckType::Tx);
            backend.trans_event_ack(&trans_event_req).await?;

            // 可能发交易
            if let Some(handles) =
                crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
            {
                handles.get_global_processed_collect_tx_handle().submit_tx(&req.trade_no).await?;
            }
        }

        Ok(())
    }

    pub async fn recover(trade_no: &str) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        ApiCollectRepo::update_api_collect_status(
            &pool,
            trade_no,
            ApiCollectStatus::Init,
            "recover",
        )
        .await?;

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let trans_event_req =
            TransEventAckReq::new(trade_no, TransType::Col, TransAckType::TxFeeRes);
        backend.trans_event_ack(&trans_event_req).await?;

        if let Some(handles) = crate::context::CONTEXT.get().unwrap().get_global_handles().upgrade()
        {
            handles.get_global_processed_collect_tx_handle().submit_tx(trade_no).await?;
        };

        Ok(())
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

        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

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
}
