use crate::{
    domain::{
        self,
        bill::BillDomain,
        chain::{adapter::MultisigAdapter, transaction::ChainTransDomain},
        task_queue::TaskQueueDomain,
    },
    infrastructure::task_queue::{self},
    messaging::mqtt::topics::MultiSignTransAcceptCompleteMsgBody,
    response_vo::multisig_transaction::ExtraData,
};
use serde_json::json;
use std::{collections::HashSet, time::Duration};
use wallet_chain_interact::tron::operations::multisig::TransactionOpt;
use wallet_database::{
    dao::{
        multisig_account::MultisigAccountDaoV1, multisig_member::MultisigMemberDaoV1,
        multisig_queue::MultisigQueueDaoV1,
    },
    entities::{
        account::{AccountEntity, QueryReq},
        multisig_account::MultisigAccountEntity,
        multisig_queue::{
            MultisigQueueData, MultisigQueueEntity, MultisigQueueSimpleEntity, MultisigQueueStatus,
            NewMultisigQueueEntity,
        },
        multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity},
        permission::PermissionWithUserEntity,
        wallet::WalletEntity,
    },
    repositories::{multisig_queue::MultisigQueueRepo, permission::PermissionRepo},
    DbPool,
};
use wallet_transport_backend::{
    api::permission::PermissionAcceptReq,
    consts::endpoint,
    request::{PermissionData, SignedTranAcceptReq, SignedTranCreateReq},
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
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let req = wallet_transport_backend::request::FindAddressRawDataReq::new_trans(
            Some(uid.to_string()),
            raw_time,
            None,
        );
        let data = backend.address_find_address_raw_data(req).await?;

        let list = data.list;
        for item in list {
            if let Some(raw_data) = item.raw_data {
                match MultisigQueueData::from_string(&raw_data) {
                    Ok(r) => {
                        let id = r.queue.id.clone();
                        if let Err(e) = Self::insert(pool.clone(), r).await {
                            tracing::error!("revover queue data error: {} queue_id = {}", e, id);
                        }
                    }
                    Err(e) => {
                        tracing::error!("revover queue data paser : {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn insert(pool: DbPool, data: MultisigQueueData) -> Result<(), crate::ServiceError> {
        let MultisigQueueData { queue, signatures } = data;
        let id = queue.id.clone();

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

        let queue = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut params).await?;
        if let Err(_e) =
            MultisigQueueRepo::sync_sign_status(&queue, params.status.to_i8(), pool.clone()).await
        {
            if !queue.permission_id.is_empty() {
                tracing::warn!(
                    "recover permission queue faild, perrmision not found and delete queue queue_id = {},permision_id = {}",
                    queue.id,queue.permission_id
                );
                MultisigQueueRepo::delete_queue(&pool, &queue.id).await?;
            }
        };

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
        Ok(backend_api.update_raw_data(queue_id, raw_data).await?)
    }

    // 波场交易队列事件、并进行默认签名
    pub async fn tron_sign_and_create_queue(
        queue: &mut NewMultisigQueueEntity,
        account: &MultisigAccountEntity,
        password: String,
        pool: DbPool,
    ) -> Result<MultisigQueueEntity, crate::ServiceError> {
        let mut members = MultisigQueueRepo::self_member_by_account(&account.id, &pool).await?;
        members.prioritize_by_address(&account.initiator_addr);

        // sign num
        let sign_num = members.0.len().min(account.threshold as usize);
        for i in 0..sign_num {
            let member = members.0.get(i).unwrap();
            let key = crate::domain::account::open_subpk_with_password(
                &queue.chain_code,
                &member.address,
                &password,
            )
            .await?;

            let sign_result = TransactionOpt::sign_transaction(&queue.raw_data, key)?;
            let sign = NewSignatureEntity::new(
                &queue.id,
                &member.address,
                &sign_result.signature,
                MultisigSignatureStatus::Approved,
                None,
            );
            queue.signatures.push(sign);
        }

        // 计算签名的状态
        queue.compute_status(account.threshold);

        let res = MultisigQueueRepo::create_queue_with_sign(pool.clone(), queue).await?;

        Ok(res)
    }

    // 非solana链对交易队列进行批量签名
    pub async fn batch_sign_queue(
        queue: &mut NewMultisigQueueEntity,
        password: &str,
        account: &MultisigAccountEntity,
        adapter: &MultisigAdapter,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        let mut members = MultisigQueueRepo::self_member_by_account(&account.id, pool).await?;
        members.prioritize_by_address(&account.initiator_addr);

        // sign num
        let sign_num = members.0.len().min(account.threshold as usize);
        for i in 0..sign_num {
            let member = members.0.get(i).unwrap();
            let key =
                ChainTransDomain::get_key(&member.address, &queue.chain_code, password, &None)
                    .await?;

            let sign_res = adapter
                .sign_multisig_tx(account, &member.address, key, &queue.raw_data)
                .await?;

            let sign = NewSignatureEntity::new_approve(
                &queue.id,
                &member.address,
                sign_res.signature,
                None,
            );
            queue.signatures.push(sign);
        }

        Ok(())
    }

    // 签名时权限创建所有的签名者(目前仅有tron)
    pub async fn batch_sign_with_permission(
        queue: &mut NewMultisigQueueEntity,
        password: &str,
        p: &PermissionWithUserEntity,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        // sign num
        let mut signatures = p
            .user
            .iter()
            .map(|u| NewSignatureEntity::from((u, queue.id.as_str())))
            .collect::<Vec<NewSignatureEntity>>();

        // 需要执行几次签名
        let sign_num = p.total_weight().min(p.permission.threshold as i32);
        let mut signed = 0;

        for user in signatures.iter_mut() {
            let query_req = QueryReq::new_address_chain(&user.address, &queue.chain_code);

            if AccountEntity::detail(pool.as_ref(), &query_req)
                .await?
                .is_some()
            {
                let key =
                    ChainTransDomain::get_key(&user.address, &queue.chain_code, password, &None)
                        .await?;

                let res = TransactionOpt::sign_transaction(&queue.raw_data, key)?;

                user.signature = res.signature;
                user.status = MultisigSignatureStatus::Approved;

                signed += user.weight.unwrap_or(1);
            }

            if signed >= sign_num {
                break;
            }
        }

        queue.signatures = signatures;

        Ok(())
    }

    // 队列数据上报到后端
    pub async fn upload_queue_backend(
        queue_id: String,
        pool: &DbPool,
        backend_params: Option<PermissionAcceptReq>,
        opt_data: Option<PermissionData>,
    ) -> Result<(), crate::ServiceError> {
        let raw_data = MultisigQueueRepo::multisig_queue_data(&queue_id, pool.clone()).await?;

        let req = SignedTranCreateReq {
            withdraw_id: raw_data.queue.id.clone(),
            address: raw_data.queue.from_addr.clone(),
            chain_code: raw_data.queue.chain_code.clone(),
            raw_data: raw_data.to_string()?,
            tx_kind: raw_data.queue.transfer_type as i8,
            permission_data: opt_data,
        };

        let mut tasks = task_queue::Tasks::new();
        let task =
            TaskQueueDomain::send_or_wrap_task(req, endpoint::multisig::SIGNED_TRAN_CREATE).await?;
        if let Some(task) = task {
            tasks = tasks.push(task);
        }

        // let task = task_queue::Task::BackendApi(task_queue::BackendApiTask::BackendApi(
        //     BackendApiTaskData::new(endpoint::multisig::SIGNED_TRAN_CREATE, &req)?,
        // ));
        // let mut tasks = task_queue::Tasks::new().push(task);
        // 多签权限的修改单独上报一份权限的数据
        if let Some(req) = backend_params {
            let task =
                TaskQueueDomain::send_or_wrap_task(req, endpoint::multisig::PERMISSION_ACCEPT)
                    .await?;
            if let Some(task) = task {
                tasks = tasks.push(task);
            }
            // let task = task_queue::Task::BackendApi(task_queue::BackendApiTask::BackendApi(
            //     BackendApiTaskData::new(endpoint::multisig::PERMISSION_ACCEPT, &backend_params)?,
            // ));
            // tasks = tasks.push(task);
        }

        tasks.send().await?;

        Ok(())
    }

    pub async fn upload_queue_sign(
        queue_id: &str,
        pool: DbPool,
        signed: Vec<NewSignatureEntity>,
        status: MultisigSignatureStatus,
    ) -> Result<(), crate::ServiceError> {
        let tx_str = signed
            .iter()
            .map(|i| i.into())
            .collect::<Vec<MultiSignTransAcceptCompleteMsgBody>>();
        let accept_address = signed.iter().map(|v| v.address.clone()).collect();

        let raw_data = MultisigQueueRepo::multisig_queue_data(queue_id, pool)
            .await?
            .to_string()?;
        let req = SignedTranAcceptReq {
            withdraw_id: queue_id.to_string(),
            tx_str: json!(tx_str),
            accept_address,
            status: status.to_i8(),
            raw_data,
        };
        TaskQueueDomain::send_or_to_queue(req, endpoint::multisig::SIGNED_TRAN_ACCEPT).await?;

        // let task = Task::BackendApi(BackendApiTask::BackendApi(BackendApiTaskData {
        //     endpoint: endpoint::multisig::SIGNED_TRAN_ACCEPT.to_string(),
        //     body: serde_func::serde_to_value(&req)?,
        // }));
        // Tasks::new().push(task).send().await?;

        Ok(())
    }

    // 过期时间 小时转换为秒
    pub fn sub_expiration(expiration: i64) -> i64 {
        if expiration == 24 {
            expiration * 3600 - 61
        } else {
            expiration * 3600
        }
    }

    pub async fn handle_queue_extra(
        queue: &MultisigQueueSimpleEntity,
        pool: &DbPool,
    ) -> Result<Option<ExtraData>, crate::ServiceError> {
        if !queue.account_id.is_empty() {
            let account =
                MultisigAccountDaoV1::find_by_id(&queue.account_id, pool.as_ref()).await?;
            if let Some(account) = account {
                return Ok(Some(ExtraData::from(account)));
            }
        } else {
            let permission = PermissionRepo::find_option(pool, &queue.permission_id).await?;

            if let Some(permission) = permission {
                return Ok(Some(ExtraData::from(permission)));
            };
        }

        Ok(None)
    }
}
