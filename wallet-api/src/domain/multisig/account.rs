use crate::{
    domain::{self, chain::adapter::ChainAdapterFactory},
    error::service::ServiceError,
    infrastructure::task_queue::{BackendApiTask, task::Tasks},
};
use sqlx::{Pool, Sqlite};
use wallet_database::{
    DbPool,
    dao::{
        multisig_account::MultisigAccountDaoV1, multisig_member::MultisigMemberDaoV1,
        multisig_queue::MultisigQueueDaoV1,
    },
    entities::{
        assets::AssetsEntity,
        coin::CoinMultisigStatus,
        multisig_account::{
            MultiAccountOwner, MultisigAccountData, MultisigAccountEntity,
            MultisigAccountPayStatus, MultisigAccountStatus, NewMultisigAccountEntity,
        },
        multisig_queue::MultisigQueueEntity,
        wallet::WalletEntity,
    },
    repositories::{ResourcesRepo, multisig_account::MultisigAccountRepo, wallet::WalletRepoTrait},
};
use wallet_transport_backend::request::FindAddressRawDataReq;
use wallet_types::constant::chain_code;

use super::MultisigQueueDomain;

pub struct MultisigDomain;

impl MultisigDomain {
    pub fn validate_queue(
        account: &MultisigAccountEntity,
    ) -> Result<(), crate::error::service::ServiceError> {
        if account.owner == MultiAccountOwner::Participant.to_i8() {
            return Err(crate::error::business::BusinessError::MultisigAccount(
                crate::error::business::multisig_account::MultisigAccountError::OnlyInitiatorCreateTx,
            ))?;
        }

        if account.pay_status != MultisigAccountPayStatus::Paid.to_i8() {
            return Err(crate::error::business::BusinessError::MultisigAccount(
                crate::error::business::multisig_account::MultisigAccountError::NotPay,
            ))?;
        }

        if account.status != MultisigAccountStatus::OnChain.to_i8() {
            return Err(crate::error::business::BusinessError::MultisigAccount(
                crate::error::business::multisig_account::MultisigAccountError::NotOnChain,
            ))?;
        }

        Ok(())
    }

    pub(crate) async fn recover_multisig_account_by_id(
        multisig_account_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        Self::recover_multisig_data_by_id(multisig_account_id).await?;
        Ok(())
    }

    // 供前端使用的目前先不使用
    pub(crate) async fn _recover_multisig_account_and_queue_data(
        repo: &mut ResourcesRepo,
        wallet_address: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let wallet = WalletRepoTrait::detail(repo, wallet_address).await?.ok_or(
            crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::Wallet(
                    crate::error::business::wallet::WalletError::NotFound,
                ),
            ),
        )?;

        MultisigDomain::recover_uid_multisig_data(&wallet.uid, None).await?;
        MultisigQueueDomain::recover_all_queue_data(&wallet.uid).await?;

        Ok(())
    }

    pub(crate) async fn recover_multisig_data_by_id(
        multisig_account_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let uid_list = WalletEntity::uid_list(&*pool)
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<std::collections::HashSet<String>>();

        MultisigDomain::recover_multisig_data(
            None,
            &uid_list,
            Some(multisig_account_id.to_string()),
            None,
        )
        .await?;
        Ok(())
    }

    // used acct_change now delete
    // pub(crate) async fn recover_multisig_data_by_address(
    //     repo: &mut ResourcesRepo,
    //     multisig_account_address: &str,
    // ) -> Result<(), crate::ServiceError> {
    //     let wallet_list = repo.wallet_list().await?;
    //     for wallet in wallet_list {
    //         MultisigDomain::recover_uid_multisig_data(
    //             &wallet.uid,
    //             Some(multisig_account_address.to_string()),
    //         )
    //         .await?;
    //     }

    //     Ok(())
    // }

    pub(crate) async fn recover_uid_multisig_data(
        uid: &str,
        filter_multisig_account_address: Option<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let uid_list = WalletEntity::uid_list(&*pool)
            .await?
            .into_iter()
            .map(|uid| uid.0)
            .collect::<std::collections::HashSet<String>>();

        MultisigDomain::recover_multisig_data(
            Some(uid.to_string()),
            &uid_list,
            None,
            filter_multisig_account_address,
        )
        .await?;
        Ok(())
    }

    // 恢复多签账号数据
    pub(crate) async fn recover_multisig_data(
        uid: Option<String>,
        uid_list: &std::collections::HashSet<String>,
        business_id: Option<String>,
        filter_multisig_account_address: Option<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let req = FindAddressRawDataReq::new_multisig(uid, business_id);
        let data = backend.address_find_address_raw_data(req).await?;

        for multisig_raw_data in data.list {
            let Some(raw_data) = multisig_raw_data.raw_data else {
                continue;
            };
            if let Err(e) = Self::handle_one_multisig_data(
                &raw_data,
                pool.clone(),
                uid_list,
                filter_multisig_account_address.clone(),
            )
            .await
            {
                tracing::error!("Recover multisig data error :{}", e);
            }
        }

        // let device_bind_address_task_data =
        //     domain::app::DeviceDomain::gen_device_bind_address_task_data().await?;
        // Tasks::new()
        //     .push(Task::BackendApi(BackendApiTask::BackendApi(
        //         device_bind_address_task_data,
        //     )))
        //     .send()
        //     .await?;
        Ok(())
    }

    pub async fn handle_one_multisig_data(
        raw_data: &str,
        pool: DbPool,
        uid_list: &std::collections::HashSet<String>,
        filter_multisig_account_address: Option<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut data = MultisigAccountData::from_string(raw_data)?;
        if let Some(multisig_account_address) = filter_multisig_account_address {
            if multisig_account_address != data.account.address {
                return Ok(());
            }
        }

        let mut flag = false;
        // handle deploy status
        if !data.account.deploy_hash.is_empty()
            && data.account.status != MultisigAccountStatus::OnChain.to_i8()
            && data.account.status != MultisigAccountStatus::OnChainFail.to_i8()
        {
            Self::handel_deploy_status(&mut data.account, false).await?;
            flag = true;
        }

        // handle pay status
        if !data.account.fee_hash.is_empty()
            && data.account.pay_status != MultisigAccountPayStatus::Paid.to_i8()
            && data.account.pay_status != MultisigAccountPayStatus::PaidFail.to_i8()
        {
            Self::handle_pay_status(&mut data.account, false).await?;
            flag = true;
        }

        let account_id = data.account.id.clone();
        let owner = Self::insert(pool.clone(), data, uid_list).await?;

        if flag && owner != MultiAccountOwner::Participant {
            Self::update_raw_data(&account_id, pool.clone()).await?;
        }

        Ok(())
    }

    pub async fn handel_deploy_status(
        data: &mut MultisigAccountEntity,
        check_expiration: bool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&data.chain_code).await?;

        // solana 多签账号来判断是否完成,其余链根据hash来判断
        match data.chain_code.as_str() {
            chain_code::SOLANA => match adapter {
                domain::chain::adapter::TransactionAdapter::Solana(solana_chain) => {
                    let addr = wallet_utils::address::parse_sol_address(&data.authority_addr)?;
                    let account = solana_chain.get_provider().account_info(addr).await?;
                    if account.value.is_some() {
                        data.status = MultisigAccountStatus::OnChain.to_i8();
                    } else if check_expiration && data.expiration_check() {
                        data.status = MultisigAccountStatus::OnChainFail.to_i8();
                    }
                }
                _ => {}
            },
            _ => {
                if let Some(tx_result) = adapter.query_tx_res(&data.deploy_hash).await? {
                    data.status = if tx_result.status == 2 {
                        MultisigAccountStatus::OnChain.to_i8()
                    } else {
                        MultisigAccountStatus::OnChainFail.to_i8()
                    }
                }
            }
        }

        Ok(())
    }

    // 同步部署中多签账号的状态
    pub async fn sync_multisig_status(
        pool: DbPool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let mut pending_account = MultisigAccountDaoV1::pending_account(pool.as_ref())
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?;

        for account in pending_account.iter_mut() {
            let status_update = account.status;
            let pay_status_up = account.pay_status;

            if account.status == MultisigAccountStatus::OnChianPending.to_i8() {
                if let Err(e) = Self::handel_deploy_status(account, true).await {
                    tracing::error!("Multisig status sync faild {}", e);
                }
            }

            if account.pay_status == MultisigAccountPayStatus::PaidPending.to_i8() {
                if let Err(e) = Self::handle_pay_status(account, true).await {
                    tracing::error!("Multisig pay status sync faild {}", e);
                }
            }

            if pay_status_up != account.pay_status || status_update != account.status {
                let rs = MultisigAccountDaoV1::update_status(
                    &account.id,
                    Some(account.status),
                    Some(account.pay_status),
                    pool.as_ref(),
                )
                .await;
                if let Err(e) = rs {
                    tracing::error!("Update status failed {}", e);
                }
            }
        }

        Ok(())
    }

    // 多签支付状态
    pub async fn handle_pay_status(
        data: &mut MultisigAccountEntity,
        check_expiration: bool,
    ) -> Result<(), crate::error::service::ServiceError> {
        if data.fee_hash == MultisigAccountEntity::NONE_TRANS_HASH {
            data.pay_status = MultisigAccountPayStatus::Paid.to_i8();
            return Ok(());
        }

        if !data.fee_chain.is_empty() && !data.fee_hash.is_empty() {
            let adapter = ChainAdapterFactory::get_transaction_adapter(&data.fee_chain).await?;
            if let Some(tx_result) = adapter.query_tx_res(&data.fee_hash).await? {
                data.pay_status = if tx_result.status == 2 {
                    MultisigAccountPayStatus::Paid.to_i8()
                } else {
                    MultisigAccountPayStatus::PaidFail.to_i8()
                }
            } else if check_expiration && data.expiration_check() {
                data.pay_status = MultisigAccountPayStatus::PaidFail.to_i8();
            }
        }
        Ok(())
    }

    pub async fn insert(
        pool: std::sync::Arc<Pool<Sqlite>>,
        data: MultisigAccountData,
        uid_list: &std::collections::HashSet<String>,
    ) -> Result<MultiAccountOwner, crate::error::service::ServiceError> {
        let MultisigAccountData { account, members } = data;
        let member_list = members.to_member_vo();

        let pay_status = MultisigAccountPayStatus::try_from(account.pay_status)?;
        let status = MultisigAccountStatus::try_from(account.status)?;

        let mut params = NewMultisigAccountEntity::new(
            Some(account.id),
            account.name,
            account.initiator_addr,
            account.address,
            account.chain_code,
            account.threshold,
            account.address_type,
            member_list,
            uid_list,
        )
        .with_authority_addr(account.authority_addr)
        .with_salt(account.salt)
        .with_deploy_hash(&account.deploy_hash)
        .with_fee_hash(&account.fee_hash)
        .with_status(status)
        .with_pay_status(pay_status);

        // 账号归属问题
        let initial_addr = params.initiator_addr.clone();

        // 判断是否是创建者
        let is_owner =
            params.member_list.iter().any(|m| m.address == initial_addr && m.is_self == 1);

        // 判断是否还有其他参与者是自己
        let has_other_self =
            params.member_list.iter().any(|m| m.address != initial_addr && m.is_self == 1);

        // 根据判断结果设置 owner
        let owner = match (is_owner, has_other_self) {
            (true, true) => MultiAccountOwner::Both,
            (true, false) => MultiAccountOwner::Owner,
            (false, _) => MultiAccountOwner::Participant,
        };
        params.owner = owner;
        params.create_at = account.created_at;
        params.fee_chain = account.fee_chain;

        if pay_status == MultisigAccountPayStatus::Paid && status == MultisigAccountStatus::OnChain
        {
            // 初始化多签资产
            domain::assets::AssetsDomain::init_default_multisig_assets(
                params.address.clone(),
                params.chain_code.clone(),
            )
            .await?;

            // 如果不是参与者,那么这个账号下所有的资产都应该被恢复为多签的
            if owner != MultiAccountOwner::Participant {
                AssetsEntity::update_tron_multisig_assets(
                    &params.address.clone(),
                    &params.chain_code.clone(),
                    CoinMultisigStatus::IsMultisig.to_i8(),
                    pool.as_ref(),
                )
                .await?;
            }
        } else if params.chain_code == chain_code::TRON
            && (status == MultisigAccountStatus::Confirmed
                || status == MultisigAccountStatus::Pending)
        {
            AssetsEntity::update_tron_multisig_assets(
                &params.address,
                &params.chain_code,
                CoinMultisigStatus::Deploying.to_i8(),
                pool.as_ref(),
            )
            .await?;
        }

        MultisigAccountDaoV1::create_account_with_member(&params, pool).await?;

        Ok(owner)
    }

    pub async fn account_by_id(
        id: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<MultisigAccountEntity, crate::error::service::ServiceError> {
        let account = MultisigAccountDaoV1::find_by_conditions(vec![("id", id)], pool.as_ref())
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?
            .ok_or(crate::error::business::BusinessError::MultisigAccount(
                crate::error::business::multisig_account::MultisigAccountError::NotFound,
            ))?;
        Ok(account)
    }
    pub async fn account_by_address(
        address: &str,
        exclude_del: bool,
        pool: &std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<MultisigAccountEntity, crate::error::service::ServiceError> {
        let mut conditions = vec![("address", address)];
        if exclude_del {
            conditions.push(("is_del", "0"));
        }

        let account = MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref())
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?
            .ok_or(crate::error::business::BusinessError::MultisigAccount(
                crate::error::business::multisig_account::MultisigAccountError::NotFound,
            ))?;
        Ok(account)
    }

    pub async fn done_account_by_address(
        address: &str,
        chain_code: &str,
        pool: &std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Option<MultisigAccountEntity>, crate::error::service::ServiceError> {
        MultisigAccountDaoV1::find_done_account(address, chain_code, &**pool)
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))
    }

    pub async fn list(
        pool: &std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let accounts = MultisigAccountDaoV1::list(vec![], pool.as_ref())
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?;
        Ok(accounts)
    }

    pub async fn queue_by_id(
        queue_id: &str,
        pool: &std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<MultisigQueueEntity, ServiceError> {
        let res = MultisigQueueDaoV1::find_by_id(queue_id, pool.as_ref())
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(e.into()))?
            .ok_or(crate::error::business::BusinessError::MultisigQueue(
                crate::error::business::multisig_queue::MultisigQueueError::NotFound,
            ))?;

        Ok(res)
    }

    pub async fn logic_delete_account(
        account_id: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<(), crate::error::service::ServiceError> {
        MultisigAccountDaoV1::logic_del_multisig_account(account_id, &*pool).await.map_err(
            |e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)),
        )?;

        wallet_database::dao::multisig_member::MultisigMemberDaoV1::logic_del_multisig_member(
            account_id, &*pool,
        )
        .await
        .map_err(|e| {
            crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
        })?;

        let queues = MultisigQueueDaoV1::logic_del_multisig_queue(account_id, &*pool)
            .await
            .map_err(|e| {
                crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
            })?
            .into_iter()
            .map(|queue| queue.id)
            .collect();
        wallet_database::dao::multisig_signatures::MultisigSignatureDaoV1::logic_del_multi_multisig_signatures(queues, &*pool)
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)))?;
        Ok(())
    }

    pub async fn physical_delete_account(
        members: &[wallet_database::entities::multisig_member::MultisigMemberEntity],
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let mut res = Vec::new();
        for member in members {
            let mut multisig_account =
                Self::physical_delete_multisig_data(&member.account_id, pool.clone()).await?;
            if let Some(multisig_account) = multisig_account.pop() {
                res.push(multisig_account);
            }
        }

        Ok(res)
    }

    // Report the successful multisig account back to the backend to update the raw data.
    pub async fn update_raw_data(
        account_id: &str,
        pool: DbPool,
    ) -> Result<(), crate::error::service::ServiceError> {
        let raw_data =
            MultisigAccountRepo::multisig_raw_data(account_id, pool).await?.to_string()?;

        let backend_api = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        Ok(backend_api.update_raw_data(account_id, raw_data).await?)
    }

    pub async fn physical_delete_wallet_account(
        members: wallet_database::entities::multisig_member::MultisigMemberEntities,
        uid: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let mut res = Vec::new();
        for member in members.0 {
            // 如果有多个钱包都参与了多签,那么不删除这个account_id的多签资产
            // 如何判断呢?
            // 查询这个account_id下所有的member， member和钱包之间有映射关系，如果钱包表中的其他钱包也参与了多签,那么不删除这个account_id的多签资产
            let account_id = member.account_id;
            // 过滤掉uid
            let uids = WalletEntity::uid_list(&*pool)
                .await?
                .into_iter()
                .filter(|u| u.0 != uid)
                .map(|uid| uid.0)
                .collect::<Vec<String>>();

            let members = MultisigMemberDaoV1::list_by_uids(&uids, &*pool).await.map_err(|e| {
                crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
            })?;
            // 如果members中有参与了多签的,那么不删除这个account_id的多签资产
            if members.iter().any(|m| m.account_id == account_id) {
                continue;
            }
            let mut multisig_account =
                Self::physical_delete_multisig_data(&account_id, pool.clone()).await?;
            if let Some(multisig_account) = multisig_account.pop() {
                res.push(multisig_account);
            }
        }

        Ok(res)
    }

    async fn physical_delete_multisig_data(
        account_id: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let multisig_account = MultisigAccountDaoV1::physical_del_multisig_account(
            account_id, &*pool,
        )
        .await
        .map_err(|e| {
            crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
        })?;

        wallet_database::dao::multisig_member::MultisigMemberDaoV1::physical_del_multisig_member(
            account_id, &*pool,
        )
        .await
        .map_err(|e| {
            crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
        })?;

        let queues = MultisigQueueDaoV1::physical_del_multisig_queue(account_id, &*pool)
            .await
            .map_err(|e| {
                crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
            })?
            .into_iter()
            .map(|queue| queue.id)
            .collect();
        wallet_database::dao::multisig_signatures::MultisigSignatureDaoV1::physical_del_multi_multisig_signatures(&*pool,queues, )
    .await
    .map_err(|e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)))?;
        Ok(multisig_account)
    }

    pub async fn physical_delete_all_account(
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Vec<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let accounts = MultisigAccountDaoV1::physical_del_multi_multisig_account(&*pool, &[])
            .await
            .map_err(|e| {
                crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
            })?;
        wallet_database::dao::multisig_member::MultisigMemberDaoV1::physical_del_multi_multisig_member(&*pool, &[])
        .await
        .map_err(|e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)))?;

        MultisigQueueDaoV1::physical_del_multi_multisig_queue(&*pool, &[]).await.map_err(|e| {
            crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
        })?;
        wallet_database::dao::multisig_signatures::MultisigSignatureDaoV1::physical_del_multi_multisig_signatures(&*pool,Vec::new() )
            .await
            .map_err(|e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)))?;
        Ok(accounts)
    }

    #[allow(dead_code)]
    pub(crate) async fn unbind_deleted_account_multisig_relations(
        deleted: &[wallet_database::entities::account::AccountEntity],
        sn: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let addresses = deleted.iter().map(|d| d.address.clone()).collect::<Vec<_>>();
        // 这个被删除的账户所关联的多签账户的成员
        let members =
            wallet_database::dao::multisig_member::MultisigMemberDaoV1::list_by_addresses(
                &addresses, &*pool,
            )
            .await
            .map_err(|e| {
                crate::error::service::ServiceError::Database(wallet_database::Error::Database(e))
            })?;

        let account_ids = members.0.iter().map(|m| m.account_id.clone()).collect::<Vec<_>>();

        let other_members = wallet_database::dao::multisig_member::MultisigMemberDaoV1::list_by_account_ids_not_addresses(
            &account_ids, &addresses, &*pool,
        )
        .await
        .map_err(|e| crate::error::service::ServiceError::Database(wallet_database::Error::Database(e)))?;

        let other_addresses = other_members.iter().map(|m| m.address.clone()).collect::<Vec<_>>();
        let other_accounts = wallet_database::entities::account::AccountEntity::list_in_address(
            &*pool,
            &other_addresses,
            None,
        )
        .await?;
        let other_members = other_members
            .0
            .into_iter()
            .filter(|m| other_accounts.iter().any(|a| a.address == m.address))
            .collect::<Vec<_>>();

        // 过滤members中有other_accounts的成员, 移除掉它们
        let should_unbind = members
            .0
            .into_iter()
            .filter(|m| !other_members.iter().any(|a| a.account_id == m.account_id))
            .collect::<Vec<_>>();
        let multisig_accounts =
            domain::multisig::MultisigDomain::physical_delete_account(&should_unbind, pool).await?;
        let device_unbind_address_task =
            domain::app::DeviceDomain::gen_device_unbind_all_address_task_data(
                deleted,
                multisig_accounts,
                sn,
            )
            .await?;

        let device_unbind_address_task = BackendApiTask::BackendApi(device_unbind_address_task);
        Tasks::new().push(device_unbind_address_task).send().await?;
        Ok(())
    }

    pub(crate) async fn check_multisig_account_exists(
        multisig_account_id: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        if MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref())
            .await
            .map_err(crate::error::service::ServiceError::Database)?
            .is_none()
        {
            tracing::warn!(
                multisig_account_id = %multisig_account_id,
                "Multisig account not found, attempting recovery"
            );
            MultisigDomain::recover_multisig_account_by_id(multisig_account_id).await?;
        }

        MultisigAccountDaoV1::find_by_id(multisig_account_id, pool.as_ref())
            .await
            .map_err(crate::error::service::ServiceError::Database)
    }
}
