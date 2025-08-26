use crate::domain::api_wallet::adapter_factory::ApiChainAdapterFactory;
use crate::domain::api_wallet::transaction::ApiChainTransDomain;
use crate::domain::api_wallet::withdraw::ApiWithdrawDomain;
use crate::messaging::notify::api_wallet::WithdrawFront;
use crate::request::api_wallet::trans::ApiTransReq;
use crate::request::transaction::TransferReq;
use crate::{
    domain::chain::transaction::ChainTransDomain,
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
    request::transaction::BaseTransferReq,
    service::transaction::TransactionService,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use wallet_database::entities::api_bill::ApiBillKind;
use wallet_database::repositories::api_withdraw::ApiWithdrawRepo;
use wallet_database::{entities::assets::AssetsId, repositories::api_assets::ApiAssetsRepo};
use wallet_utils::conversion;

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransMsg {
    from: String,
    to: String,
    value: String,
    #[serde(rename = "chain")]
    chain_code: String,
    #[serde(rename = "token_addr")]
    token_address: String,
    #[serde(rename = "token_code")]
    symbol: String,
    trade_no: String,
    // 交易类型： 1 提币 / 2 归集
    trade_type: u8,
    uid: String,
}

// 归集和提币
impl TransMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let backend_api = crate::Context::get_global_backend_api()?;
        // // 查询策略
        let strategy = backend_api.query_collection_strategy(&self.uid).await?;
        let threshold = strategy.threshold;

        let Some(chain_config) = strategy
            .chain_configs
            .iter()
            .find(|config| config.chain_code == self.chain_code)
        else {
            return Err(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::ChainConfigNotFound(self.chain_code.to_owned()),
            )
            .into());
        };

        // 查询手续费
        let mut params = BaseTransferReq::new(
            &self.from,
            &self.to.to_string(),
            &self.value.to_string(),
            &self.chain_code.to_string(),
            &self.symbol.to_string(),
        );
        params.with_token(Some(self.token_address.clone()));

        let fee = TransactionService::transaction_fee(params).await?;

        // 查询资产余额
        let main_coin = ChainTransDomain::main_coin(&self.chain_code).await?;

        let main_symbol = main_coin.symbol;
        let assets_id = AssetsId::new(&self.from, &self.chain_code, &main_symbol, None);
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        // 如果手续费不足，则从其他地址转入手续费费用
        if conversion::decimal_from_str(&assets.balance)?
            < conversion::decimal_from_str(&fee.content)?
        {
            // 查询出款地址余额主币余额
            let assets_id = AssetsId::new(
                &chain_config.normal_address.address,
                &self.chain_code,
                &main_symbol,
                None,
            );
            let withdraw_assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
                .await?
                .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;
            let withdraw_balance = withdraw_assets.balance;
            // 出款地址余额够不够
            // if 不够 -> 出款地址余额 -> 不够 -> 通知后端：失败原因
            if conversion::decimal_from_str(&withdraw_balance)?
                < conversion::decimal_from_str(&fee.content)?
            {
                todo!()
            }
            // else 够 -> 转账手续费 -> 通知后端手续费转账
            else {
                // 获取密码缓存
                let password = "";
                // 从出款地址转手续费到from_addr
                let base = BaseTransferReq::new(
                    &self.from,
                    &self.to,
                    &self.value,
                    &self.chain_code,
                    &self.symbol,
                );
                let params = TransferReq {
                    base,
                    password: password.to_string(),
                    fee_setting:
                        r#"{"gasLimit":23100,"baseFee":"0","priorityFee":"1000000000","maxFeePerGas":"1000000000"}"#
                            .to_string(),
                    signer:None,
                };
                // ApiChainTransDomain::transfer(params, bill_kind, adapter)
                todo!()
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

    pub(crate) async fn withdraw(&self) -> Result<(), crate::ServiceError> {
        // 验证金额是否需要输入密码

        let token_address = if self.token_address.is_empty() {
            None
        } else {
            Some(self.token_address.clone())
        };
        let req = ApiTransReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type,
        };
        ApiWithdrawDomain::withdraw(&req).await
    }
}
