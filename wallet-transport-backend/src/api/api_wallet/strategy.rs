use crate::{
    request::api_wallet::strategy::*,
    response_vo::api_wallet::strategy::{CollectionStrategyResp, WithdrawStrategyResp},
};

use super::BackendApi;

impl BackendApi {
    // 保存&更新归集策略配置
    pub async fn save_or_update_collection_strategy(
        &self,
        req: &SaveOrUpdateCollectionStrategyReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 保存&更新出款策略配置
    pub async fn save_or_update_withdraw_strategy(
        &self,
        req: &SaveOrUpdateWithdrawStrategyReq,
    ) -> Result<Option<()>, crate::Error> {
        todo!()
    }

    // 查询归集策略配置
    pub async fn query_collection_strategy(
        &self,
        req: &QueryCollectionStrategyReq,
    ) -> Result<CollectionStrategyResp, crate::Error> {
        todo!()
    }

    // 查询出款策略配置
    pub async fn query_withdraw_strategy(
        &self,
        req: &QueryWithdrawStrategyReq,
    ) -> Result<WithdrawStrategyResp, crate::Error> {
        todo!()
    }
}
