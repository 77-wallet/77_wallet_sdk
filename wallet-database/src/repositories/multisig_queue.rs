use super::ResourcesRepo;
use crate::{
    dao::{
        multisig_account::MultisigAccountDaoV1, multisig_member::MultisigMemberDaoV1,
        multisig_queue::MultisigQueueDaoV1, multisig_signatures::MultisigSignatureDaoV1,
        permission::PermissionDao, permission_user::PermissionUserDao,
    },
    entities::{
        multisig_member::MultisigMemberEntities,
        multisig_queue::{
            fail_reason::SIGN_FAILED, MemberSignedResult, MultisigQueueData, MultisigQueueEntity,
            MultisigQueueSimpleEntity, MultisigQueueStatus, NewMultisigQueueEntity,
        },
        multisig_signatures::{
            MultisigSignatureEntities, MultisigSignatureStatus, NewSignatureEntity,
        },
    },
    pagination::Pagination,
    DbPool,
};
use sqlx::{Pool, Sqlite};

pub struct MultisigQueueRepo {
    repo: ResourcesRepo,
}
impl MultisigQueueRepo {
    pub fn new(db_pool: crate::DbPool) -> Self {
        Self {
            repo: ResourcesRepo::new(db_pool),
        }
    }
}

impl MultisigQueueRepo {
    pub async fn create_queue_with_sign(
        pool: DbPool,
        params: &mut NewMultisigQueueEntity,
    ) -> Result<MultisigQueueEntity, crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // create multisig queue
        let res = MultisigQueueDaoV1::create_queue(params, tx.as_mut()).await?;

        //  if signatures is not empty insert signatures
        if !params.signatures.is_empty() {
            for signature in &mut params.signatures {
                signature.queue_id = res.id.clone();

                let exists = MultisigSignatureDaoV1::find_signature(
                    &signature.queue_id,
                    &signature.address,
                    tx.as_mut(),
                )
                .await?;

                match exists {
                    Some(s) => {
                        if !s.signature.is_empty() {
                            MultisigSignatureDaoV1::update_status(signature, tx.as_mut()).await?
                        }
                    }
                    None => {
                        MultisigSignatureDaoV1::create_signature(signature, tx.as_mut()).await?
                    }
                };
            }
        }

        tx.commit()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;
        Ok(res)
    }

    // 拼接额外的信息(区分多签账号和权限)
    pub async fn find_by_id_with_extra(
        id: &str,
        pool: &DbPool,
    ) -> Result<Option<MultisigQueueSimpleEntity>, crate::Error> {
        Ok(MultisigQueueDaoV1::find_with_extra(id, pool.as_ref()).await?)
    }

    pub async fn queue_list(
        from: Option<&str>,
        chain_code: Option<&str>,
        status: i32,
        page: i64,
        page_size: i64,
        pool: DbPool,
    ) -> Result<Pagination<MultisigQueueSimpleEntity>, crate::Error> {
        let lists =
            MultisigQueueDaoV1::lists(from, chain_code, status, page, page_size, pool.clone())
                .await?;

        Ok(lists)
    }

    pub async fn find_by_id(
        pool: &DbPool,
        queue_id: &str,
    ) -> Result<Option<MultisigQueueEntity>, crate::Error> {
        Ok(MultisigQueueDaoV1::find_by_id(queue_id, pool.as_ref()).await?)
    }

    pub async fn update_fail(
        pool: &DbPool,
        queue_id: &str,
        reason: &str,
    ) -> Result<(), crate::Error> {
        Ok(MultisigQueueDaoV1::update_fail(queue_id, reason, pool.as_ref()).await?)
    }

    pub async fn signed_result(
        queue_id: &str,
        account_id: &str,
        permission_id: &str,
        pool: DbPool,
    ) -> Result<Vec<MemberSignedResult>, crate::Error> {
        if !account_id.is_empty() {
            Self::member_signed_result(account_id, queue_id, pool).await
        } else {
            Self::permission_signed_result(permission_id, queue_id, pool).await
        }
    }

    // 多签账号的前面结果
    pub async fn member_signed_result(
        account_id: &str,
        queue_id: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MemberSignedResult>, crate::Error> {
        let mut result = vec![];

        let mut member = MultisigMemberDaoV1::find_records_by_id(account_id, pool.as_ref()).await?;

        for item in member.0.iter_mut() {
            let mut sign_result =
                MemberSignedResult::new(&item.name, &item.address, item.is_self, 1);

            // 获取签名的结果
            let sign =
                MultisigSignatureDaoV1::find_signature(queue_id, &item.address, pool.as_ref())
                    .await?;
            if let Some(sign) = sign {
                sign_result.singed = sign.status;
                sign_result.signature = sign.signature;
            }
            result.push(sign_result);
        }
        Ok(result)
    }

    // 权限的签名结果
    pub async fn permission_signed_result(
        permission_id: &str,
        queue_id: &str,
        pool: DbPool,
    ) -> Result<Vec<MemberSignedResult>, crate::Error> {
        let mut result = vec![];

        let mut users = PermissionUserDao::find_by_permission(permission_id, pool.as_ref()).await?;

        for user in users.iter_mut() {
            let mut sign_result =
                MemberSignedResult::new("", &user.address, user.is_self as i8, user.weight);

            // 获取签名的结果
            let sign =
                MultisigSignatureDaoV1::find_signature(queue_id, &user.address, pool.as_ref())
                    .await?;
            if let Some(sign) = sign {
                sign_result.singed = sign.status;
                sign_result.signature = sign.signature;
            }
            result.push(sign_result);
        }
        Ok(result)
    }

    pub async fn create_or_update_sign(
        params: &NewSignatureEntity,
        pool: &DbPool,
    ) -> Result<(), crate::Error> {
        let signature = MultisigSignatureDaoV1::find_signature(
            &params.queue_id,
            &params.address,
            pool.as_ref(),
        )
        .await?;

        match signature {
            Some(s) => {
                if !s.signature.is_empty() {
                    MultisigSignatureDaoV1::update_status(params, pool.as_ref()).await?
                }
            }
            None => MultisigSignatureDaoV1::create_signature(params, pool.as_ref()).await?,
        };
        Ok(())
    }

    // 未执行的交易修改状态(根据签名,数量)
    pub async fn sync_sign_status(
        queue: &MultisigQueueEntity,
        status: i8,
        pool: crate::DbPool,
    ) -> Result<(), crate::Error> {
        let status = MultisigQueueStatus::from_i8(status);

        if !status.need_sync_status() {
            return Ok(());
        }

        // 多签的账号或者权限的账号
        let status = if !queue.account_id.is_empty() {
            MultisigQueueRepo::compute_status_by_account(queue, &pool).await?
        } else {
            MultisigQueueRepo::compute_status_by_permission(queue, &pool).await?
        };

        match status {
            MultisigQueueStatus::Fail => {
                MultisigQueueDaoV1::update_fail(&queue.id, SIGN_FAILED, pool.as_ref()).await?
            }
            _ => MultisigQueueDaoV1::update_status(&queue.id, status, pool.as_ref()).await?,
        }

        Ok(())
    }

    // 根据多签账号计算队列里面的状态
    async fn compute_status_by_account(
        queue: &MultisigQueueEntity,
        pool: &DbPool,
    ) -> Result<MultisigQueueStatus, crate::Error> {
        let account = MultisigAccountDaoV1::find_by_id(&queue.account_id, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        // fetch all member sign result
        let signed =
            MultisigQueueRepo::member_signed_result(&queue.account_id, &queue.id, pool.clone())
                .await?;

        Ok(Self::compute_status(signed, account.threshold as i64))
    }

    fn compute_status(signed: Vec<MemberSignedResult>, threshold: i64) -> MultisigQueueStatus {
        let mut status = MultisigQueueStatus::HasSignature;
        let mut remain_num = 0;
        let mut approved_num = 0;

        for sign in signed {
            if sign.is_self == 1 && sign.singed == MultisigSignatureStatus::UnSigned.to_i8() {
                status = MultisigQueueStatus::PendingSignature;
            }

            // 剩余未签名的数量
            if sign.singed == MultisigSignatureStatus::UnSigned.to_i8() {
                remain_num += sign.weight;
            }

            // 已经同意签名的数量
            if sign.singed == MultisigSignatureStatus::Approved.to_i8() {
                approved_num += sign.weight;
            }
        }

        if approved_num >= threshold {
            status = MultisigQueueStatus::PendingExecution;
        } else {
            // 如果剩余的未签名数量 + 同意签名的数量 < 阈值 则这个交易队列失败
            if remain_num + approved_num < threshold {
                status = MultisigQueueStatus::Fail;
            }
        }
        status
    }

    async fn compute_status_by_permission(
        queue: &MultisigQueueEntity,
        pool: &DbPool,
    ) -> Result<MultisigQueueStatus, crate::Error> {
        let permission = PermissionDao::find_by_id(&queue.permission_id, false, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        // fetch all user sign result
        let signed = MultisigQueueRepo::signed_result(
            &queue.id,
            &queue.account_id,
            &queue.permission_id,
            pool.clone(),
        )
        .await?;

        Ok(Self::compute_status(signed, permission.threshold))
    }

    pub async fn self_member_account_id(
        &mut self,
        id: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::get_self_by_id(id, &*self.repo.db_pool).await?)
    }

    pub async fn self_member_by_account(
        id: &str,
        pool: &DbPool,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::get_self_by_id(id, pool.as_ref()).await?)
    }

    pub async fn get_signed_list(
        pool: &DbPool,
        queue_id: &str,
    ) -> Result<MultisigSignatureEntities, crate::Error> {
        Ok(MultisigSignatureDaoV1::get_signed_list(queue_id, pool.as_ref()).await?)
    }

    pub async fn update_status_and_hash(
        &mut self,
        queue_id: &str,
        status: MultisigQueueStatus,
        tx_hash: &str,
    ) -> Result<(), crate::Error> {
        Ok(MultisigQueueDaoV1::update_status_and_tx_hash(
            queue_id,
            status,
            tx_hash,
            &*self.repo.db_pool,
        )
        .await?)
    }

    pub async fn multisig_queue_data(
        queue_id: &str,
        pool: crate::DbPool,
    ) -> Result<MultisigQueueData, crate::Error> {
        let queue = MultisigQueueDaoV1::find_by_id(queue_id, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        let signatures = MultisigSignatureDaoV1::find_by_queue_id(queue_id, pool).await?;

        Ok(MultisigQueueData::new(
            queue,
            MultisigSignatureEntities(signatures),
        ))
    }

    pub async fn permission_update_fail(address: &str, pool: &DbPool) -> Result<(), crate::Error> {
        Ok(MultisigQueueDaoV1::permission_fail(address, pool.as_ref()).await?)
    }

    pub async fn ongoing_queue(
        chain_code: &str,
        address: &str,
        pool: &DbPool,
    ) -> Result<Option<MultisigQueueEntity>, crate::Error> {
        let queue = MultisigQueueDaoV1::ongoing_queue(pool.as_ref(), chain_code, address).await?;
        Ok(queue)
    }
}
