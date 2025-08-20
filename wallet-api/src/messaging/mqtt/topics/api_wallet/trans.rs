use crate::domain::api_wallet::adapter_factory::ApiChainAdapterFactory;
use crate::domain::api_wallet::transaction::ApiChainTransDomain;
use crate::messaging::notify::api_wallet::WithdrawFront;
use crate::request::transaction::TransferReq;
use crate::{
    domain::chain::transaction::ChainTransDomain,
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
    request::transaction::BaseTransferReq,
    service::transaction::TransactionService,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use wallet_database::entities::api_bill::ApiBillKind;
use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;
use wallet_database::{
    entities::assets::AssetsId,
    repositories::api_assets::ApiAssetsRepo,
};
use wallet_transport_backend::request::api_wallet::strategy::QueryCollectionStrategyReq;
use wallet_utils::conversion;

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransMsg {
    from: String,
    to: String,
    value: String,
    chain: String,
    token_addr: String,
    token_code: String,
    trade_no: String,
    trade_type: u8,
    uid: String,
    chain_code: String,
    symbol: String,
}

// 归集
impl TransMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let backend_api = crate::Context::get_global_backend_api()?;
        let req = QueryCollectionStrategyReq::new();
        // 查询策略
        let strategy = backend_api.query_collection_strategy(&req).await?;

        // 校验手续费
        let mut params = BaseTransferReq::new(
            &self.from,
            &self.to.to_string(),
            &self.value.to_string(),
            &self.chain.to_string(),
            &self.token_code.to_string(),
        );
        params.with_token(Some(self.token_addr.clone()));

        let fee = TransactionService::transaction_fee(params).await?;

        // 如果手续费不足，则从其他地址转入手续费费用

        let main_coin = ChainTransDomain::main_coin(&self.chain).await?;

        let main_symbol = main_coin.symbol;
        let assets_id = AssetsId::new(&self.from, &self.chain, &main_symbol, None);
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        //
        if conversion::decimal_from_str(&assets.balance)?
            < conversion::decimal_from_str(&fee.content)?
        {
            // 查询出款地址余额主币余额
            // let withdraw_address = ;
            let balance = "1";
            // 出款地址余额够不够
            // if 不够 -> 出款地址余额 -> 不够 -> 通知后端：失败原因
            if conversion::decimal_from_str(balance)? < conversion::decimal_from_str(&fee.content)?
            {
            }
            // else 够 -> 转账手续费 -> 通知后端手续费转账
            else {
                // 从出款地址转手续费到from_addr
            }
        }
        // 执行归集
        else {
            // 上链
            // 生成订单
        }

        // ApiAccountDomain::address_used(&self.chain_code, self.index, &self.uid, None).await?;

        // let data = NotifyEvent::AddressUse(self.to_owned());
        // FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }

    pub(crate) async fn withdraw(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        ApiWithdrawRepo::upsert_api_withdraw(
            &pool,
            &self.uid,
            "",
            &self.from,
            &self.to,
            &self.value,
            &self.token_addr,
            &self.symbol,
            &self.trade_no,
            self.trade_type,
        ).await?;

        let data = NotifyEvent::Withdraw(WithdrawFront {
            uid: self.uid.to_string(),
            from_addr: self.from.to_string(),
            to_addr: self.to.to_string(),
            value: self.value.to_string(),
        });
        FrontendNotifyEvent::new(data).send().await?;

        // 可能发交易
        let value = Decimal::from_str(&self.value).unwrap();
        if (value < Decimal::from(10)) {
            let mut params = BaseTransferReq::new(
                &self.from,
                &self.to.to_string(),
                &self.value.to_string(),
                &self.chain.to_string(),
                &self.token_code.to_string(),
            );
            params.with_token(Some(self.token_addr.clone()));

            let req = TransferReq {
                base: params,
                password: "".to_string(),
                fee_setting: "".to_string(),
                signer: None,
            };
            // 发交易
            let adapter = ApiChainAdapterFactory::get_transaction_adapter(self.chain_code.as_str()).await?;
            let tx_hash = ApiChainTransDomain::transfer(req, ApiBillKind::Transfer, &adapter).await?;
        }
        Ok(())
    }
}
