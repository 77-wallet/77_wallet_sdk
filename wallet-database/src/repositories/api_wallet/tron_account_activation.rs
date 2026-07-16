use crate::{
    ApiResourceDbPool,
    dao::api_tron_account_activation::ApiTronAccountActivationDao,
    entities::api_tron_account_activation::{
        ApiTronAccountActivationEntity, NewApiTronAccountActivation,
    },
};

pub struct ApiTronAccountActivationRepo;

impl ApiTronAccountActivationRepo {
    pub async fn upsert(
        pool: &ApiResourceDbPool,
        input: NewApiTronAccountActivation,
    ) -> Result<(), crate::Error> {
        ApiTronAccountActivationDao::upsert(pool.write_ref(), input).await
    }

    pub async fn find_by_activation_trade_no(
        pool: &ApiResourceDbPool,
        activation_trade_no: &str,
    ) -> Result<Option<ApiTronAccountActivationEntity>, crate::Error> {
        ApiTronAccountActivationDao::find_by_activation_trade_no(
            pool.read_ref(),
            activation_trade_no,
        )
        .await
    }

    pub async fn find_by_origin_target(
        pool: &ApiResourceDbPool,
        origin_trade_no: &str,
        origin_trade_type: i64,
        target_address: &str,
    ) -> Result<Option<ApiTronAccountActivationEntity>, crate::Error> {
        ApiTronAccountActivationDao::find_by_origin_target(
            pool.read_ref(),
            origin_trade_no,
            origin_trade_type,
            target_address,
        )
        .await
    }

    pub async fn mark_account_verified(
        pool: &ApiResourceDbPool,
        activation_trade_no: &str,
    ) -> Result<u64, crate::Error> {
        ApiTronAccountActivationDao::mark_account_verified(pool.write_ref(), activation_trade_no)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entities::{
            api_trade_type::ApiTradeType,
            api_tron_account_activation::{
                ApiTronAccountActivationStatus, NewApiTronAccountActivation,
            },
        },
        repositories::test_helper::setup_api_resource_pool,
    };

    #[tokio::test]
    async fn tron_account_activation_upsert_is_idempotent_by_origin_target() {
        let pool = setup_api_resource_pool("tron_account_activation_upsert").await;

        ApiTronAccountActivationRepo::upsert(
            &pool,
            NewApiTronAccountActivation::collect(
                "uid_1",
                "collect_1",
                "act_collect_1",
                "T_payer_1",
                "T_target_1",
            ),
        )
        .await
        .unwrap();
        ApiTronAccountActivationRepo::upsert(
            &pool,
            NewApiTronAccountActivation::collect(
                "uid_1",
                "collect_1",
                "act_collect_1",
                "T_payer_2",
                "T_target_1",
            ),
        )
        .await
        .unwrap();

        let got = ApiTronAccountActivationRepo::find_by_origin_target(
            &pool,
            "collect_1",
            ApiTradeType::Collect as i64,
            "T_target_1",
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(got.activation_trade_no, "act_collect_1");
        assert_eq!(got.payer_address, "T_payer_2");
        assert_eq!(got.amount, "0.000001");
        assert_eq!(got.status, ApiTronAccountActivationStatus::Pending);
        assert!(got.account_verified_at.is_none());
    }

    #[tokio::test]
    async fn tron_account_activation_account_verified_is_idempotent() {
        let pool = setup_api_resource_pool("tron_account_activation_verified").await;

        ApiTronAccountActivationRepo::upsert(
            &pool,
            NewApiTronAccountActivation::collect(
                "uid_1",
                "collect_verified",
                "act_collect_verified",
                "T_payer",
                "T_target",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            ApiTronAccountActivationRepo::mark_account_verified(&pool, "act_collect_verified")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            ApiTronAccountActivationRepo::mark_account_verified(&pool, "act_collect_verified")
                .await
                .unwrap(),
            0
        );

        let got = ApiTronAccountActivationRepo::find_by_activation_trade_no(
            &pool,
            "act_collect_verified",
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(got.status, ApiTronAccountActivationStatus::Success);
        assert!(got.account_verified_at.is_some());
    }
}
