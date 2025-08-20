use wallet_database::{
    entities::assets::AssetsId,
    repositories::{api_assets::ApiAssetsRepo, chain::ChainRepo},
};
use wallet_transport_backend::request::api_wallet::strategy::QueryCollectionStrategyReq;
use wallet_utils::conversion;

use crate::{
    domain::{api_wallet::account::ApiAccountDomain, chain::transaction::ChainTransDomain},
    messaging::notify::{FrontendNotifyEvent, event::NotifyEvent},
    request::transaction::BaseTransferReq,
    service::transaction::TransactionService,
};

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
}
