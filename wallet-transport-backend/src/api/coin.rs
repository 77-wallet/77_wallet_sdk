use crate::{
    request::{SwapTokenQueryReq, TokenQueryByPageReq},
    response_vo::coin::{CoinInfos, TokenPopularByPages, TokenPrice, TokenPriceInfos},
};

use super::BackendApi;

impl BackendApi {
    pub async fn custom_token_init(
        &self,

        req: crate::request::CustomTokenInitReq,
    ) -> Result<bool, crate::Error> {
        let res = self
            .client
            .post("token/custom/token/init")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn token_query_price(
        &self,

        req: crate::request::TokenQueryPriceReq,
    ) -> Result<TokenPriceInfos, crate::Error> {
        let res = self
            .client
            .post("token/queryPrice")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn token_query_by_page(
        &self,

        req: &TokenQueryByPageReq,
    ) -> Result<CoinInfos, crate::Error> {
        let req = serde_json::json!(req);

        let res = self
            .client
            .post("token/queryByPage")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn query_history_price(
        &self,

        req: &crate::request::TokenQueryHistoryPrice,
    ) -> Result<crate::response_vo::coin::TokenHistoryPrices, crate::Error> {
        let res = self
            .client
            .post("token/queryHisPrice")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn query_popular_by_page(
        &self,

        req: &crate::request::TokenQueryPopularByPageReq,
    ) -> Result<TokenPopularByPages, crate::Error> {
        let res = self
            .client
            .post("token/queryPopularByPage")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn token_balance_refresh(
        &self,
        req: crate::request::TokenBalanceRefreshReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("token/balance/refresh")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(&self.aes_cbc_cryptor)
    }
    // 单独查询token价格
    pub async fn token_price(
        &self,
        chain_code: &str,
        token_addr: &str,
    ) -> Result<TokenPrice, crate::Error> {
        let endpoint = "token/collect/token";

        let req = serde_json::json!({
            "chainCode": chain_code,
            "tokenAddress": token_addr
        });

        let token = self.post_request::<_, TokenPrice>(endpoint, &req).await?;

        Ok(token)
    }

    pub async fn swap_token_list(
        &self,
        req: SwapTokenQueryReq,
    ) -> Result<TokenPopularByPages, crate::Error> {
        let endpoint = "token/swapTokenList";

        let token = self
            .post_request::<_, TokenPopularByPages>(endpoint, &req)
            .await?;

        Ok(token)
    }
}
// pub async fn token_subscribe(
//     &self,

//     req: crate::request::TokenSubscribeReq,
// ) -> Result<HashMap<String, serde_json::Value>, crate::Error> {
//     let res = self
//         .client
//         .post("token/subscribe")
//         .json(serde_json::json!(req))
//         .send::<serde_json::Value>()
//         .await?;
//     let res: crate::response::BackendResponse =
//         wallet_utils::serde_func::serde_from_value(res)?;
//     res.process(&self.aes_cbc_cryptor)
// }

// pub async fn token_query_by_contract_address(
//     &self,

//     req: &TokenQueryByContractAddressReq,
// ) -> Result<TokenQueryByContractAddressRes, crate::Error> {
//     let req = serde_json::json!(req);

//     let res = self
//         .client
//         .post("token/queryByContractAddress")
//         .json(req)
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }

// pub async fn token_cancel_subscribe(
//     &self,

//     req: crate::request::TokenCancelSubscribeReq,
// ) -> Result<HashMap<String, serde_json::Value>, crate::Error> {
//     let res = self
//         .client
//         .post("token/cancelSubscribe")
//         .json(serde_json::json!(req))
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }

// query token fee_rate
// pub async fn token_query_by_currency(
//     &self,

//     chain_code: &str,
//     currency: &str,
//     symbol: &str,
// ) -> Result<crate::response_vo::coin::TokenCurrency, crate::Error> {
//     let mut params = HashMap::new();

//     let symbol = symbol.to_lowercase();
//     params.insert("chainCode", chain_code);
//     params.insert("code", &symbol);
//     params.insert("currency", currency);

//     let res = self
//         .client
//         .post("token/queryByCurrency")
//         .json(params)
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }

// pub async fn token_query_by_contract_address(
//     &self,

//     req: &TokenQueryByContractAddressReq,
// ) -> Result<TokenQueryByContractAddressRes, crate::Error> {
//     let req = serde_json::json!(req);

//     let res = self
//         .client
//         .post("token/queryByContractAddress")
//         .json(req)
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }

// pub async fn token_cancel_subscribe(
//     &self,

//     req: crate::request::TokenCancelSubscribeReq,
// ) -> Result<HashMap<String, serde_json::Value>, crate::Error> {
//     let res = self
//         .client
//         .post("token/cancelSubscribe")
//         .json(serde_json::json!(req))
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }

// query token fee_rate
// pub async fn token_query_by_currency(
//     &self,

//     chain_code: &str,
//     currency: &str,
//     symbol: &str,
// ) -> Result<crate::response_vo::coin::TokenCurrency, crate::Error> {
//     let mut params = HashMap::new();

//     let symbol = symbol.to_lowercase();
//     params.insert("chainCode", chain_code);
//     params.insert("code", &symbol);
//     params.insert("currency", currency);

//     let res = self
//         .client
//         .post("token/queryByCurrency")
//         .json(params)
//         .send::<crate::response::BackendResponse>()
//         .await?;
//     res.process(&self.aes_cbc_cryptor)
// }
