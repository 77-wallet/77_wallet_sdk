use crate::entities::node::{NodeCreateVo, NodeEntity};

#[async_trait::async_trait]
pub trait NodeRepoTrait: super::TransactionTrait {
    async fn add(&mut self, input: NodeCreateVo) -> Result<NodeEntity, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::upsert, input)
    }

    async fn list(&mut self, is_local: Option<u8>) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::list, &[], is_local, None)
    }

    async fn list_by_chain(
        &mut self,
        chain_code: &[String],
        is_local: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::list, chain_code, is_local, None)
    }

    async fn get_local_node_by_chain(
        &mut self,
        chain_code: &str,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(
            executor,
            NodeEntity::list,
            &[chain_code.to_string()],
            Some(1),
            Some(1)
        )
    }

    async fn get_node_list_in_chain_codes(
        &mut self,
        chain_codes: &[String],
        status: Option<u8>,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::list, chain_codes, None, status)
    }

    async fn detail(&mut self, node_id: &str) -> Result<Option<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        let req = crate::entities::node::QueryReq::new(node_id);
        crate::execute_with_executor!(executor, NodeEntity::detail, &req)
    }

    async fn delete(
        &mut self,
        // rpc_url: &str,
        // chain_code: &str,
        node_id: &str,
    ) -> Result<Vec<NodeEntity>, crate::Error> {
        let executor = self.get_conn_or_tx()?;
        crate::execute_with_executor!(executor, NodeEntity::delete, node_id)
    }
}
