use crate::{CoreDbPool, dao::multisig_signatures::MultisigSignatureDaoV1};

pub struct MultisigSignatureRepo;

impl MultisigSignatureRepo {
    pub async fn logic_delete_by_queue_ids(
        pool: &CoreDbPool,
        queue_ids: Vec<String>,
    ) -> Result<(), crate::Error> {
        MultisigSignatureDaoV1::logic_del_multi_multisig_signatures(queue_ids, pool.as_ref())
            .await?;
        Ok(())
    }

    pub async fn physical_delete_by_queue_ids(
        pool: &CoreDbPool,
        queue_ids: Vec<String>,
    ) -> Result<(), crate::Error> {
        MultisigSignatureDaoV1::physical_del_multi_multisig_signatures(pool.as_ref(), queue_ids)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MultisigSignatureRepo;
    use crate::{
        dao::multisig_signatures::MultisigSignatureDaoV1,
        entities::multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity},
        repositories::test_helper::setup_core_pool,
    };

    fn build_signature(queue_id: &str, address: &str) -> NewSignatureEntity {
        NewSignatureEntity::new(
            queue_id,
            address,
            "0xsignature",
            MultisigSignatureStatus::Approved,
            None,
        )
    }

    #[tokio::test]
    async fn multisig_signature_repo_logic_delete_by_queue_ids_success() {
        let pool = setup_core_pool("wallet_db_multisig_signature_repo_success").await;
        MultisigSignatureDaoV1::create_signature(&build_signature("q_success", "T_signer"), pool.as_ref())
            .await
            .unwrap();

        MultisigSignatureRepo::logic_delete_by_queue_ids(&pool, vec!["q_success".to_string()])
            .await
            .unwrap();

        let is_del: i64 = sqlx::query_scalar(
            "SELECT is_del FROM multisig_signatures WHERE queue_id = ? AND address = ?",
        )
        .bind("q_success")
        .bind("T_signer")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
        assert_eq!(is_del, 1);
    }

    #[tokio::test]
    async fn multisig_signature_repo_delete_unknown_queue_keeps_existing_row() {
        let pool = setup_core_pool("wallet_db_multisig_signature_repo_edge").await;
        MultisigSignatureDaoV1::create_signature(&build_signature("q_keep", "T_signer_keep"), pool.as_ref())
            .await
            .unwrap();

        MultisigSignatureRepo::logic_delete_by_queue_ids(&pool, vec!["q_missing".to_string()])
            .await
            .unwrap();

        let is_del: i64 = sqlx::query_scalar(
            "SELECT is_del FROM multisig_signatures WHERE queue_id = ? AND address = ?",
        )
        .bind("q_keep")
        .bind("T_signer_keep")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
        assert_eq!(is_del, 0);
    }

    #[tokio::test]
    async fn multisig_signature_repo_tx_rollback_keeps_is_del_unchanged() {
        let pool = setup_core_pool("wallet_db_multisig_signature_repo_rollback").await;
        MultisigSignatureDaoV1::create_signature(&build_signature("q_rb", "T_signer_rb"), pool.as_ref())
            .await
            .unwrap();

        let mut tx = pool.as_ref().begin().await.unwrap();
        MultisigSignatureDaoV1::logic_del_multi_multisig_signatures(
            vec!["q_rb".to_string()],
            tx.as_mut(),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();

        let is_del: i64 = sqlx::query_scalar(
            "SELECT is_del FROM multisig_signatures WHERE queue_id = ? AND address = ?",
        )
        .bind("q_rb")
        .bind("T_signer_rb")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
        assert_eq!(is_del, 0);
    }
}
