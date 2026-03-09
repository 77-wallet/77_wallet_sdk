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
