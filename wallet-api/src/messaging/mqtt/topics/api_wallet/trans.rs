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
    // 交易类型： 1 提币 / 2 归集 / 3 归集手续费交易
    trade_type: u8,
    uid: String,
}

// 归集和提币
impl TransMsg {
    pub(crate) async fn exec(&self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        match self.trade_type {
            1 => self.withdraw().await?,
            2 => self.collect().await?,
            3 => self.transfer_fee().await?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) async fn transfer_fee(&self) -> Result<(), crate::ServiceError> {
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
        Ok(())
    }

    pub(crate) async fn collect(&self) -> Result<(), crate::ServiceError> {
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
            trade_type: self.trade_type,
        };
        ApiCollectDomain::collect(&req).await
    }

    pub(crate) async fn withdraw(&self) -> Result<(), crate::ServiceError> {
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
            trade_type: self.trade_type,
        };
        ApiWithdrawDomain::withdraw(&req).await
    }
}
