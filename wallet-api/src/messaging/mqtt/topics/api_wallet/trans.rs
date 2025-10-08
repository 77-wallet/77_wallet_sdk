use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;
use wallet_transport_backend::request::api_wallet::transaction::ServiceFeeUploadReq;

use crate::{
    domain::{
        api_wallet::{
            trans::{ApiTransDomain, collect::ApiCollectDomain, withdraw::ApiWithdrawDomain},
            wallet::ApiWalletDomain,
        },
        coin::CoinDomain,
    },
    request::api_wallet::trans::{ApiBaseTransferReq, ApiTransferReq, ApiWithdrawReq},
};
use crate::domain::api_wallet::trans::fee::ApiFeeDomain;
use crate::request::api_wallet::trans::ApiTransferFeeReq;

// biz_type = RECHARGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmOrderTransMsg {
    from: String,
    to: String,
    value: String,
    #[serde(rename = "chain")]
    chain_code: String,
    #[serde(rename = "tokenAddr")]
    token_address: String,
    #[serde(rename = "tokenCode")]
    symbol: String,
    /// 平台交易单号
    trade_no: String,
    /// 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    trade_type: u32,
    /// 是否需要审核（可空）： 1 不需要审核 / 2 需要审核
    #[serde(deserialize_with = "wallet_utils::serde_func::string_to_u32")]
    audit: u32,
    uid: String,
}

// 归集和提币
impl AwmOrderTransMsg {
    pub(crate) async fn exec(
        &self,
        _msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            3 => self.transfer_fee().await?,
            _ => {}
        }
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(_msg_id);
        let res = backend.msg_ack(msg_ack_req).await;
        tracing::info!("withdraw wallet transfer from {} to {} value {:?}", self.from, self.to, res);
        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::error::service::ServiceError> {
        // 获取密码缓存
        let password = ApiWalletDomain::get_passwd().await?;

        // 从出款地址转手续费到from_addr
        tracing::info!("打手续费 fee: {}", self.value);
        // todo!();
        let coin =
            CoinDomain::get_coin(&self.chain_code, &self.symbol, Some(self.token_address.clone()))
                .await?;
        let mut params =
            ApiBaseTransferReq::new(&self.from, &self.to, &self.value, &self.chain_code);
        params.with_token(None, coin.decimals, &coin.symbol);

        let transfer_req = ApiTransferReq { base: params, password: password.to_string() };
        // 上链
        // 发交易
        let _tx_resp = ApiTransDomain::transfer(transfer_req).await?;
        // ApiChainTransDomain::transfer(params, bill_kind, adapter)
        // let _resource_consume = if tx_resp.consumer.is_none() {
        //     "0".to_string()
        // } else {
        //     tx_resp.consumer.unwrap().energy_used.to_string()
        // };
        tracing::info!(
            "withdraw wallet transfer fee {} to {} value {}",
            self.from,
            self.to,
            self.value
        );

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let req = ServiceFeeUploadReq::new(
            &self.trade_no,
            &self.chain_code,
            &self.symbol,
            &self.token_address,
            &self.from,
            &self.to,
            wallet_utils::unit::string_to_f64(&self.value)?,
        );
        backend.upload_service_fee_record(&req).await?;
        Ok(())
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::error::service::ServiceError> {
        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiWithdrawReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
            audit: self.audit,
        };
        ApiCollectDomain::collect_v2(&req).await
    }

    pub(crate) async fn transfer_fee_v2(&self) -> Result<(), crate::error::service::ServiceError> {
        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiTransferFeeReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
        };
        let res = ApiFeeDomain::transfer_fee(&req).await;
        tracing::info!("transfer fee wallet transfer fee {} to {} value {:?}", self.from, self.to, res);
        Ok(())
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::error::service::ServiceError> {
        // 验证金额是否需要输入密码

        let token_address =
            if self.token_address.is_empty() { None } else { Some(self.token_address.clone()) };
        let req = ApiWithdrawReq {
            uid: self.uid.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            value: self.value.to_string(),
            chain_code: self.chain_code.to_string(),
            token_address,
            symbol: self.symbol.to_string(),
            trade_no: self.trade_no.to_string(),
            trade_type: self.trade_type as u8,
            audit: self.audit,
        };
        let res = ApiWithdrawDomain::withdraw(&req).await;
        tracing::info!("withdraw wallet transfer fee {} to {} value {:?}", self.from, self.to, res);
        Ok(())
    }
}
