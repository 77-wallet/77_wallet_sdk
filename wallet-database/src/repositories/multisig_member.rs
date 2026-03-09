use crate::{
    CoreDbPool, dao::multisig_member::MultisigMemberDaoV1,
    entities::multisig_member::{MultisigMemberEntities, MultisigMemberEntity},
};

pub struct MultisigMemberRepo;

impl MultisigMemberRepo {
    pub async fn list_by_uid(
        pool: &CoreDbPool,
        uid: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_uid(uid, pool.as_ref()).await?)
    }

    pub async fn list_by_uids(
        pool: &CoreDbPool,
        uids: &[String],
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_uids(uids, pool.as_ref()).await?)
    }

    pub async fn list_by_addresses(
        pool: &CoreDbPool,
        addresses: &[String],
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_addresses(addresses, pool.as_ref()).await?)
    }

    pub async fn list_by_account_ids_not_addresses(
        pool: &CoreDbPool,
        account_ids: &[String],
        addresses: &[String],
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_account_ids_not_addresses(
            account_ids,
            addresses,
            pool.as_ref(),
        )
        .await?)
    }

    pub async fn logic_delete_by_account_id(
        pool: &CoreDbPool,
        account_id: &str,
    ) -> Result<(), crate::Error> {
        MultisigMemberDaoV1::logic_del_multisig_member(account_id, pool.as_ref()).await?;
        Ok(())
    }

    pub async fn physical_delete_by_account_id(
        pool: &CoreDbPool,
        account_id: &str,
    ) -> Result<Vec<MultisigMemberEntity>, crate::Error> {
        Ok(MultisigMemberDaoV1::physical_del_multisig_member(account_id, pool.as_ref()).await?)
    }

    pub async fn physical_delete_by_account_ids(
        pool: &CoreDbPool,
        account_ids: &[&str],
    ) -> Result<Vec<MultisigMemberEntity>, crate::Error> {
        Ok(MultisigMemberDaoV1::physical_del_multi_multisig_member(pool.as_ref(), account_ids)
            .await?)
    }
}
