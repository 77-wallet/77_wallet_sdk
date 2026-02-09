use crate::{
    DbPool, dao::multisig_member::MultisigMemberDaoV1,
    entities::multisig_member::MultisigMemberEntities,
};

pub struct MultisigMemberRepo;

impl MultisigMemberRepo {
    pub async fn list_by_uid(
        pool: &DbPool,
        uid: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_uid(uid, pool.as_ref()).await?)
    }
}
