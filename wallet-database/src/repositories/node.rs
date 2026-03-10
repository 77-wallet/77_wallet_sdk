use crate::{
    CoreDbPool,
    dao::node::NodeDao,
    entities::node::{NodeCreateVo, NodeEntity},
};

pub struct NodeRepo;

impl NodeRepo {
    pub async fn get_local_node_by_chain(
        pool: &CoreDbPool,
        chain_code: &str,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeDao::list(pool.as_ref(), &[chain_code.to_string()], Some(1), Some(1)).await?)
    }

    pub async fn list(
        pool: &CoreDbPool,
        is_local: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeDao::list(pool.as_ref(), &[], is_local, None).await?)
    }

    pub async fn list_with_network(
        pool: &CoreDbPool,
        is_local: Option<u8>,
        network: Option<&str>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeDao::list_with_network(pool.as_ref(), &[], is_local, None, network).await?)
    }

    pub async fn list_by_chain_with_network(
        pool: &CoreDbPool,
        chain_code: &str,
        network: Option<&str>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeDao::list_with_network(
            pool.as_ref(),
            &[chain_code.to_string()],
            None,
            None,
            network,
        )
        .await?)
    }

    pub async fn upsert(pool: &CoreDbPool, req: NodeCreateVo) -> Result<NodeEntity, crate::Error> {
        Ok(NodeDao::upsert(pool.as_ref(), req).await?)
    }

    pub async fn detail(
        pool: &CoreDbPool,
        node_id: &str,
    ) -> Result<Option<NodeEntity>, crate::Error> {
        let executor = pool.as_ref();
        Ok(NodeDao::detail_by_node_id(executor, node_id).await?)
    }

    pub async fn disable_backend_not_in(
        pool: &CoreDbPool,
        chain_code: &str,
        backend_ids: &[String],
    ) -> Result<u64, crate::Error> {
        let executor = pool.as_ref();
        NodeDao::disable_backend_not_in(executor, chain_code, backend_ids).await
    }

    pub async fn get_node_list_in_chain_codes(
        pool: &CoreDbPool,
        chain_codes: &[String],
        status: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        Ok(NodeDao::list(pool.as_ref(), chain_codes, None, status).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeRepo;
    use crate::{dao::node::NodeDao, entities::node::NodeCreateVo, repositories::test_helper::setup_core_pool};

    fn build_node(node_id: &str, chain_code: &str) -> NodeCreateVo {
        NodeCreateVo::new(node_id, "node_name", chain_code, "https://rpc.test", None)
            .with_network("mainnet")
            .with_is_local(1)
    }

    #[tokio::test]
    async fn node_repo_upsert_and_detail_success() {
        let pool = setup_core_pool("wallet_db_node_repo_success").await;
        NodeRepo::upsert(&pool, build_node("node_success", "tron")).await.unwrap();

        let found = NodeRepo::detail(&pool, "node_success").await.unwrap().unwrap();
        assert_eq!(found.node_id, "node_success");
        assert_eq!(found.chain_code, "tron");
    }

    #[tokio::test]
    async fn node_repo_missing_node_returns_none() {
        let pool = setup_core_pool("wallet_db_node_repo_edge").await;
        let found = NodeRepo::detail(&pool, "node_missing").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn node_repo_tx_rollback_keeps_node_absent() {
        let pool = setup_core_pool("wallet_db_node_repo_rollback").await;

        let mut tx = pool.as_ref().begin().await.unwrap();
        NodeDao::upsert(tx.as_mut(), build_node("node_rb", "tron")).await.unwrap();
        tx.rollback().await.unwrap();

        let found = NodeRepo::detail(&pool, "node_rb").await.unwrap();
        assert!(found.is_none());
    }
}
