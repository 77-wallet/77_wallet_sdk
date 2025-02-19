use std::collections::HashMap;

use crate::{
    request::{TokenQueryByContractAddressReq, TokenQueryByPageReq},
    response_vo::coin::{CoinInfos, TokenPriceInfos, TokenQueryByContractAddressRes, TokenRates},
};

use super::BackendApi;

impl BackendApi {
    pub async fn custom_token_init(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::CustomTokenInitReq,
    ) -> Result<bool, crate::Error> {
        let res = self
            .client
            .post("token/custom/token/init")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_subscribe(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::TokenSubscribeReq,
    ) -> Result<HashMap<String, serde_json::Value>, crate::Error> {
        let res = self
            .client
            .post("token/subscribe")
            .json(serde_json::json!(req))
            .send::<serde_json::Value>()
            .await?;
        let res: crate::response::BackendResponse =
            wallet_utils::serde_func::serde_from_value(res)?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_query_price(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::TokenQueryPriceReq,
    ) -> Result<TokenPriceInfos, crate::Error> {
        let res = self
            .client
            .post("token/queryPrice")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn _token_query_price(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::TokenQueryPriceReq,
    ) -> Result<wallet_types::valueobject::TokenPopularByPages, crate::Error> {
        let res = self
            .client
            .post("token/queryPrice")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_rates(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
    ) -> Result<TokenRates, crate::Error> {
        let res = self
            .client
            .post("token/queryRates")
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_query_by_page(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &TokenQueryByPageReq,
    ) -> Result<CoinInfos, crate::Error> {
        let req = serde_json::json!(req);

        let res = self
            .client
            .post("token/queryByPage")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_query_by_contract_address(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &TokenQueryByContractAddressReq,
    ) -> Result<TokenQueryByContractAddressRes, crate::Error> {
        let req = serde_json::json!(req);

        let res = self
            .client
            .post("token/queryByContractAddress")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn token_cancel_subscribe(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: crate::request::TokenCancelSubscribeReq,
    ) -> Result<HashMap<String, serde_json::Value>, crate::Error> {
        let res = self
            .client
            .post("token/cancelSubscribe")
            .json(serde_json::json!(req))
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    /// query token fee_rate
    pub async fn token_query_by_currency(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        chain_code: &str,
        currency: &str,
        symbol: &str,
    ) -> Result<crate::response_vo::coin::TokenCurrency, crate::Error> {
        let mut params = HashMap::new();

        let symbol = symbol.to_lowercase();
        params.insert("chainCode", chain_code);
        params.insert("code", &symbol);
        params.insert("currency", currency);

        let res = self
            .client
            .post("token/queryByCurrency")
            .json(params)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn query_history_price(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::TokenQueryHistoryPrice,
    ) -> Result<crate::response_vo::coin::TokenHistoryPrices, crate::Error> {
        // let mut params = HashMap::new();

        // let symbol = symbol.to_lowercase();
        // params.insert("chainCode", chain_code);
        // params.insert("code", &symbol);
        // params.insert("dateType", date_type);
        // params.insert("currency", currency);

        let res = self
            .client
            .post("token/queryHisPrice")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }

    pub async fn query_popular_by_page(
        &self,
        aes_cbc_cryptor: &wallet_utils::cbc::AesCbcCryptor,
        req: &crate::request::TokenQueryPopularByPageReq,
    ) -> Result<wallet_types::valueobject::TokenPopularByPages, crate::Error> {
        let res = self
            .client
            .post("token/queryPopularByPage")
            .json(req)
            .send::<crate::response::BackendResponse>()
            .await?;
        res.process(aes_cbc_cryptor)
    }
}
