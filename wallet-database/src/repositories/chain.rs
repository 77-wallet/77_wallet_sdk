use crate::{
    CoreDbPool,
    dao::chain::ChainDao,
    entities::chain::{ChainCreateVo, ChainEntity, ChainWithNode},
};

pub struct ChainRepo;

impl ChainRepo {
    pub async fn detail(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::detail(pool.read_ref(), chain_code).await?)
    }

    pub async fn delete(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::delete(pool.write_ref(), chain_code).await?)
    }

    pub async fn add(pool: &CoreDbPool, input: ChainCreateVo) -> Result<ChainEntity, crate::Error> {
        Ok(ChainDao::upsert(pool.write_ref(), input).await?)
    }

    pub async fn get_chain_list(pool: &CoreDbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::list(pool.read_ref(), Some(1)).await?)
    }

    pub async fn get_chain_list_v2(pool: &CoreDbPool) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::list_v2(pool.read_ref(), Some(1)).await?)
    }

    pub async fn get_chain_node_list(
        pool: &CoreDbPool,
    ) -> Result<Vec<ChainWithNode>, crate::Error> {
        Ok(ChainDao::list_with_node_info(pool.read_ref()).await?)
    }

    pub async fn detail_with_node(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Option<ChainWithNode>, crate::Error> {
        Ok(ChainDao::chain_node_info(pool.read_ref(), chain_code).await?)
    }

    pub async fn detail_with_main_symbol(
        pool: &CoreDbPool,
        main_symbol: &str,
    ) -> Result<Option<ChainEntity>, crate::Error> {
        Ok(ChainDao::detail_with_main_symbol(pool.read_ref(), main_symbol).await?)
    }

    pub async fn toggle_chains_status(
        pool: &CoreDbPool,
        chain_codes: &[String],
    ) -> Result<Vec<ChainEntity>, crate::Error> {
        Ok(ChainDao::toggle_chains_status(pool.write_ref(), chain_codes).await?)
    }

    pub async fn upsert_multi_chain(
        pool: &CoreDbPool,
        input: Vec<ChainCreateVo>,
    ) -> Result<(), crate::Error> {
        ChainDao::upsert_multi_chain(pool.write_ref(), input).await
    }

    pub async fn user_select(
        pool: &CoreDbPool,
        chain_code: &str,
        node_id: &str,
    ) -> Result<(), crate::Error> {
        Ok(ChainDao::user_select(pool.write_ref(), chain_code, node_id).await?)
    }

    pub async fn set_chain_node_with_type(
        pool: &crate::CoreDbPool,
        chain_code: &str,
        node_id: &str,
        bind_type: crate::entities::api_chain::NodeBindType,
    ) -> Result<(), crate::Error> {
        Ok(ChainDao::set_chain_node_with_type(pool.write_ref(), chain_code, node_id, bind_type)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::ChainRepo;
    use crate::{
        dao::chain::ChainDao,
        entities::{api_chain::NodeBindType, chain::ChainCreateVo},
        repositories::test_helper::setup_core_pool,
    };

    fn build_chain(chain_code: &str) -> ChainCreateVo {
        ChainCreateVo::new(
            "Tron",
            chain_code,
            &[String::from("tron")],
            NodeBindType::AutoBackend,
            "TRX",
        )
    }

    #[tokio::test]
    async fn chain_repo_add_and_detail_success() {
        let pool = setup_core_pool("wallet_db_chain_repo_success").await;
        ChainRepo::add(&pool, build_chain("tron_success")).await.unwrap();

        let found = ChainRepo::detail(&pool, "tron_success").await.unwrap().unwrap();
        assert_eq!(found.chain_code, "tron_success");
        assert_eq!(found.main_symbol, "TRX");
    }

    #[tokio::test]
    async fn chain_repo_missing_chain_returns_none() {
        let pool = setup_core_pool("wallet_db_chain_repo_edge").await;
        let found = ChainRepo::detail(&pool, "chain_missing").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn chain_repo_tx_rollback_keeps_chain_absent() {
        let pool = setup_core_pool("wallet_db_chain_repo_rollback").await;

        let mut tx = pool.write_ref().begin().await.unwrap();
        ChainDao::upsert(tx.as_mut(), build_chain("tron_rb")).await.unwrap();
        tx.rollback().await.unwrap();

        let found = ChainRepo::detail(&pool, "tron_rb").await.unwrap();
        assert!(found.is_none());
    }
}
