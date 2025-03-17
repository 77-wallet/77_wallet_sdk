use crate::domain::{self, bill::BillDomain, multisig::MultisigDomain};
use std::{collections::HashSet, sync::Arc, time::Duration};
use wallet_database::{
    dao::{multisig_member::MultisigMemberDaoV1, multisig_queue::MultisigQueueDaoV1},
    entities::{
        multisig_queue::{
            MultisigQueueData, MultisigQueueEntity, MultisigQueueStatus, NewMultisigQueueEntity,
        },
        multisig_signatures::NewSignatureEntity,
        wallet::WalletEntity,
    },
    repositories::multisig_queue::MultisigQueueRepo,
    DbPool,
};

pub struct MultisigQueueDomain;
impl MultisigQueueDomain {
    pub fn validate_queue(
        queue: &MultisigQueueEntity,
        execute: bool,
    ) -> Result<(), crate::ServiceError> {
        // check queue is expired
        let time = sqlx::types::chrono::Utc::now().timestamp();
        if queue.expiration < time {
            return Err(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::Expired,
            ))?;
        }

        // check status
        if queue.status == MultisigQueueStatus::InConfirmation.to_i8() {
            return Err(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::AlreadyExecuted,
            ))?;
        }

        if queue.status == MultisigQueueStatus::Fail.to_i8() {
            return Err(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::FailedQueue,
            ))?;
        }

        // execute multisig tx need check status
        if execute && queue.status != MultisigQueueStatus::PendingExecution.to_i8() {
            return Err(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::NotPendingExecStatus,
            ))?;
        }

        Ok(())
    }

    pub async fn recover_all_uid_queue_data() -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let uid_list = WalletEntity::uid_list(&*pool)
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<Vec<String>>();

        let raw_time = Self::get_raw_time(&uid_list).await?;

        for uid in uid_list {
            Self::recover_queue_data_with_raw_time(&uid, raw_time.clone()).await?;
        }

        Ok(())
    }

    pub async fn recover_all_queue_data(uid: &str) -> Result<(), crate::ServiceError> {
        let raw_time = Self::get_raw_time(&[uid.to_string()]).await?;
        Self::recover_queue_data_with_raw_time(uid, raw_time).await?;
        Ok(())
    }

    pub(crate) async fn get_raw_time(
        uid_list: &[String],
    ) -> Result<Option<String>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account_ids = MultisigMemberDaoV1::list_by_uids(uid_list, &*pool)
            .await
            .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?
            .0
            .into_iter()
            .map(|member| member.account_id)
            .collect::<HashSet<String>>();

        let account_ids_vec: Vec<String> = account_ids.into_iter().collect();
        let queue = MultisigQueueDaoV1::list_by_account_ids(&account_ids_vec, &*pool)
            .await
            .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?;

        let raw_time = queue.map(|q| {
            let now = q.created_at - Duration::from_secs(86400);
            now.format("%Y-%m-%d %H:%M:%S").to_string()
        });
        Ok(raw_time)
    }

    pub async fn recover_queue_data(uid: &str) -> Result<(), crate::ServiceError> {
        let raw_time = Self::get_raw_time(&[uid.to_string()]).await?;

        Self::recover_queue_data_with_raw_time(uid, raw_time).await?;

        Ok(())
    }

    pub(crate) async fn recover_queue_data_with_raw_time(
        uid: &str,
        raw_time: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let req = wallet_transport_backend::request::FindAddressRawDataReq::new_trans(
            Some(uid.to_string()),
            raw_time,
            None,
        );
        let data = backend.address_find_address_raw_data(cryptor, req).await?;

        let list = data.list;
        for item in list {
            if let Some(raw_data) = item.raw_data {
                if let Ok(multisig_queue_data) = MultisigQueueData::from_string(&raw_data) {
                    let que_id = multisig_queue_data.queue.id.clone();
                    if let Err(e) = Self::insert(pool.clone(), multisig_queue_data).await {
                        tracing::error!("revover queue data error: {} queue_id = {}", e, que_id);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn insert(
        pool: Arc<sqlx::Pool<sqlx::Sqlite>>,
        data: MultisigQueueData,
    ) -> Result<(), crate::ServiceError> {
        let MultisigQueueData { queue, signatures } = data;
        let id = queue.id.clone();
        let account_id = queue.account_id.clone();

        // 构建交易数据
        let mut params = NewMultisigQueueEntity::from(queue).check_expiration();

        let mut report = false;
        if !params.tx_hash.is_empty() && params.status == MultisigQueueStatus::InConfirmation {
            let tx =
                domain::bill::BillDomain::get_onchain_bill(&params.tx_hash, &params.chain_code)
                    .await?;
            if let Some(tx) = tx {
                if tx.status == 2 {
                    params.status = MultisigQueueStatus::Success;
                } else {
                    params.status = MultisigQueueStatus::Fail;
                }
            }
            report = true;
        }

        for sig in signatures.0 {
            let signature = NewSignatureEntity::try_from(sig)?;
            params = params.with_signatures(signature);
        }

        MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut params).await?;

        let multisig_account = MultisigDomain::account_by_id(&account_id, pool.clone()).await?;

        MultisigQueueRepo::sync_sign_status(
            &id,
            &account_id,
            multisig_account.threshold,
            params.status.to_i8(),
            pool.clone(),
        )
        .await?;

        if report {
            Self::update_raw_data(&id, pool.clone()).await?;
        }

        Ok(())
    }

    // For transactions in the confirmation queue, periodically query the transaction results.
    pub async fn sync_queue_status(queue_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let queue = MultisigQueueDaoV1::find_by_id(queue_id, pool.as_ref())
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        let Some(queue) = queue else {
            return Ok(());
        };

        // 仅处理状态为 InConfirmation 且有 tx_hash 的队列
        if queue.status != MultisigQueueStatus::InConfirmation.to_i8() || queue.tx_hash.is_empty() {
            return Ok(());
        }

        // 获取链上交易状态
        if let Some(rs) = BillDomain::get_onchain_bill(&queue.tx_hash, &queue.chain_code).await? {
            // 更新状态：2 为成功，否则为失败
            let tx_status = if rs.status == 2 {
                MultisigQueueStatus::Success
            } else {
                MultisigQueueStatus::Fail
            };

            MultisigQueueDaoV1::update_status(queue_id, tx_status, pool.as_ref())
                .await
                .map_err(|e| crate::ServiceError::Database(e.into()))?;

            Self::update_raw_data(queue_id, pool).await?;
        }

        Ok(())
    }

    // Report the successful transaction queue back to the backend to update the raw data.
    pub async fn update_raw_data(queue_id: &str, pool: DbPool) -> Result<(), crate::ServiceError> {
        let raw_data = MultisigQueueRepo::multisig_queue_data(queue_id, pool)
            .await?
            .to_string()?;

        let backend_api = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        Ok(backend_api
            .update_raw_data(cryptor, queue_id, raw_data)
            .await?)
    }
}
