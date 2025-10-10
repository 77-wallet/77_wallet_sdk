use crate::{
    consts::endpoint::api_wallet::{
        TRANS_STRATEGY_COLLECT_SAVE, TRANS_STRATEGY_GET_COLLECT_CONFIG,
        TRANS_STRATEGY_GET_WITHDRAWAL_CONFIG, TRANS_STRATEGY_WITHDRAWAL_SAVE,
    },
    request::api_wallet::strategy::*
    ,
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};
use std::collections::HashMap;

use crate::api::BackendApi;
use crate::api_request::ApiBackendRequest;
use crate::api_response::ApiBackendResponse;

impl BackendApi {
    // 保存&更新归集策略配置
    pub async fn save_collect_strategy(
        &self,
        req: &SaveCollectStrategyReq,
    ) -> Result<Option<()>, crate::Error> {
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_STRATEGY_COLLECT_SAVE)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;

        res.process()
    }

    // 保存&更新出款策略配置
    pub async fn save_withdrawal_strategy(
        &self,
        req: &SaveWithdrawStrategyReq,
    ) -> Result<Option<()>, crate::Error> {
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_STRATEGY_WITHDRAWAL_SAVE)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        res.process()
    }

    // 查询归集策略配置
    pub async fn query_collect_strategy(
        &self,
        uid: &str,
    ) -> Result<CollectionStrategyResp, crate::Error> {
        let mut req = HashMap::new();
        req.insert("uid", uid);
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_STRATEGY_GET_COLLECT_CONFIG)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;
        res.process()
    }

    // 查询出款策略配置
    pub async fn query_withdrawal_strategy(
        &self,
        uid: &str,
    ) -> Result<WithdrawStrategyResp, crate::Error> {
        let mut req = HashMap::new();
        req.insert("uid", uid);
        let api_req = ApiBackendRequest::new(req)?;
        let res = self
            .client
            .post(TRANS_STRATEGY_GET_WITHDRAWAL_CONFIG)
            .json(api_req)
            .send::<ApiBackendResponse>()
            .await?;

        res.process()
    }
}
