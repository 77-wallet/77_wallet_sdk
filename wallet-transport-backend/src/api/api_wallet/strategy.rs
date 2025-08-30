use crate::{
    consts::endpoint::api_wallet::{
        TRANS_SERVICE_FEE_TRANS, TRANS_STRATEGY_GET_COLLECT_CONFIG,
        TRANS_STRATEGY_GET_WITHDRAWAL_CONFIG, TRANS_STRATEGY_WITHDRAWAL_SAVE,
    },
    request::api_wallet::strategy::*,
    response::BackendResponse,
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

use super::BackendApi;

impl BackendApi {
    // 保存&更新归集策略配置
    pub async fn save_collection_strategy(
        &self,
        req: &Strategy,
    ) -> Result<Option<()>, crate::Error> {
        let res =
            self.client.post(TRANS_SERVICE_FEE_TRANS).json(req).send::<BackendResponse>().await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 保存&更新出款策略配置
    pub async fn save_withdraw_strategy(
        &self,
        req: &SaveWithdrawStrategyReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post(TRANS_STRATEGY_WITHDRAWAL_SAVE)
            .json(req)
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 查询归集策略配置
    pub async fn query_collection_strategy(
        &self,
        uid: &str,
    ) -> Result<CollectionStrategyResp, crate::Error> {
        let res = self
            .client
            .post(TRANS_STRATEGY_GET_COLLECT_CONFIG)
            .json(serde_json::json!({
                "uid": uid,
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }

    // 查询出款策略配置
    pub async fn query_withdraw_strategy(
        &self,
        uid: &str,
    ) -> Result<WithdrawStrategyResp, crate::Error> {
        let res = self
            .client
            .post(TRANS_STRATEGY_GET_WITHDRAWAL_CONFIG)
            .json(serde_json::json!({
                "uid": uid,
            }))
            .send::<BackendResponse>()
            .await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
