use crate::domain::chain::adapter::{ChainAdapterFactory, MultisigAdapter};
use crate::domain::chain::transaction::ChainTransaction;
use crate::domain::chain::TransferResp;
use crate::domain::multisig::{MultisigDomain, MultisigQueueDomain};
use crate::infrastructure::task_queue::{
    BackendApiTask, BackendApiTaskData, CommonTask, Task, Tasks,
};
use crate::request::transaction::Signer;
use crate::response_vo::multisig_account::QueueInfo;
use crate::response_vo::MultisigQueueFeeParams;
use crate::response_vo::{multisig_transaction::MultisigQueueInfoVo, transaction::TransferParams};
use crate::{domain, response_vo};
use wallet_chain_interact::sol::operations::SolInstructionOperation;
use wallet_chain_interact::tron::operations::TronConstantOperation as _;
use wallet_chain_interact::{btc, eth, sol, tron, BillResourceConsume};
use wallet_database::dao::multisig_member::MultisigMemberDaoV1;
use wallet_database::dao::multisig_queue::MultisigQueueDaoV1;
use wallet_database::entities::bill::{BillKind, NewBillEntity};
use wallet_database::entities::multisig_queue::{
    fail_reason, MultisigQueueEntity, MultisigQueueStatus, NewMultisigQueueEntity, QueueTaskEntity,
};
use wallet_database::entities::multisig_signatures::{MultisigSignatureStatus, NewSignatureEntity};
use wallet_database::pagination::Pagination;
use wallet_database::repositories::multisig_queue::MultisigQueueRepo;
use wallet_database::repositories::permission::PermissionRepo;
use wallet_database::DbPool;
use wallet_transport_backend::consts::endpoint;
use wallet_transport_backend::request::{PermissionData, SignedTranUpdateHashReq};
use wallet_types::constant::chain_code;
use wallet_utils::{serde_func, unit};

pub struct MultisigTransactionService;

impl MultisigTransactionService {
    pub async fn create_queue_fee(
        req_params: MultisigQueueFeeParams,
    ) -> Result<response_vo::EstimateFeeResp, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let account = MultisigDomain::account_by_address(&req_params.from, true, &pool).await?;

        let assets =
            ChainTransaction::assets(&req_params.chain_code, &req_params.symbol, &req_params.from)
                .await?;

        let main_coin = ChainTransaction::main_coin(&assets.chain_code).await?;

        let adapter = ChainAdapterFactory::get_multisig_adapter(&account.chain_code).await?;

        let res = adapter
            .build_multisig_fee(
                &req_params,
                &account,
                assets.decimals,
                assets.token_address(),
                &main_coin.symbol,
            )
            .await?;
        let fee_resp =
            response_vo::EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), res);

        Ok(fee_resp)
    }

    // Creates a new multisig transaction queue.
    pub async fn create_multisig_queue(
        req: TransferParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        if let Some(signer) = req.signer.clone() {
            Self::create_with_permission(req, &password, signer).await
        } else {
            Self::create_with_account(req, &password).await
        }
    }

    async fn create_with_account(
        req: TransferParams,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets = ChainTransaction::assets(&req.chain_code, &req.symbol, &req.from).await?;

        let account = MultisigDomain::account_by_address(&req.from, true, &pool).await?;
        MultisigDomain::validate_queue(&account)?;

        let key =
            ChainTransaction::get_key(&req.from, &req.chain_code, &password, &req.signer).await?;

        let adapter = ChainAdapterFactory::get_multisig_adapter(&account.chain_code).await?;
        let rs = adapter
            .build_multisig_with_account(&req, &account, &assets, key)
            .await?;

        let mut queue = NewMultisigQueueEntity::from(&req)
            .with_msg_hash(&rs.tx_hash)
            .with_raw_data(&rs.raw_data)
            .with_token(assets.token_address())
            .set_id();
        queue.account_id = account.id.clone();

        if queue.chain_code != chain_code::SOLANA {
            // 对多签队列进行签名
            MultisigQueueDomain::batch_sign_queue(&mut queue, &password, &account, &adapter, &pool)
                .await?;
        }

        queue.compute_status(account.threshold);

        // write multisig queue data to local database
        let res = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut queue).await?;

        // 上报后端
        MultisigQueueDomain::upload_queue_backend(res.id, &pool, None, None).await?;

        Ok(rs.tx_hash)
    }

    async fn create_with_permission(
        req: TransferParams,
        password: &str,
        signer: Signer,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets = ChainTransaction::assets(&req.chain_code, &req.symbol, &req.from).await?;

        let permission =
            PermissionRepo::permission_with_user(&pool, &req.from, signer.permission_id, false)
                .await?;

        let Some(p) = permission else {
            return Err(crate::BusinessError::Permission(
                crate::PermissionError::ActivesPermissionNotFound,
            ))?;
        };

        let adapter = ChainAdapterFactory::get_multisig_adapter(&req.chain_code).await?;
        let rs = adapter
            .build_multisig_with_permission(&req, &p.permission, &assets)
            .await?;

        let mut queue = NewMultisigQueueEntity::from(&req)
            .with_msg_hash(&rs.tx_hash)
            .with_raw_data(&rs.raw_data)
            .with_token(assets.token_address())
            .set_id();
        queue.permission_id = p.permission.id.clone();

        if queue.chain_code != chain_code::SOLANA {
            // 对多签队列进行签名
            MultisigQueueDomain::batch_sign_with_permission(&mut queue, &password, &p, &pool)
                .await?;
        }

        queue.compute_status(p.permission.threshold as i32);

        // write multisig queue data to local database
        let res = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut queue).await?;

        let opt = PermissionData {
            opt_address: signer.address.clone(),
            users: p.users(),
        };

        // 上报后端
        MultisigQueueDomain::upload_queue_backend(res.id, &pool, None, Some(opt)).await?;

        Ok(rs.tx_hash)
    }

    pub async fn multisig_queue_list(
        from: Option<&str>,
        chain_code: Option<&str>,
        status: i32,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<MultisigQueueInfoVo>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        // 先处理过期的交易
        let _ = MultisigQueueDaoV1::update_expired_queue(pool.as_ref()).await;

        let mut lists =
            MultisigQueueRepo::queue_list(from, chain_code, status, page, page_size, pool.clone())
                .await?;

        let mut task = Tasks::new();
        let mut data = vec![];
        // every queue signed num
        for item in lists.data.iter_mut() {
            // if queue status has exec , add to task query result
            if item.status == MultisigQueueStatus::InConfirmation.to_i8() {
                task = task.push(Task::Common(CommonTask::QueryQueueResult(
                    QueueTaskEntity {
                        id: item.id.clone(),
                        status: item.status,
                    },
                )));
            }

            let signature = MultisigQueueRepo::signed_result(
                &item.id,
                &item.account_id,
                &item.permission_id,
                pool.clone(),
            )
            .await?;

            let sign_num: i64 = signature
                .iter()
                .filter_map(|sig| if sig.singed != 0 { Some(1) } else { None })
                .sum();
            let extra = MultisigQueueDomain::handle_queue_extra(item, &pool).await?;

            data.push(MultisigQueueInfoVo {
                queue: item.clone(),
                extra: extra.unwrap_or_default(),
                signature,
                sign_num,
            });
        }

        task.send().await?;

        let result = Pagination {
            page: lists.page,
            page_size: lists.page_size,
            total_count: lists.total_count,
            data,
        };

        Ok(result)
    }

    // queue info
    pub async fn multisig_queue_info(
        queue_id: &str,
    ) -> Result<MultisigQueueInfoVo, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let queue = MultisigQueueRepo::find_by_id_with_extra(queue_id, &pool)
            .await?
            .ok_or(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::NotFound,
            ))?;

        let signature = MultisigQueueRepo::signed_result(
            &queue.id,
            &queue.account_id,
            &queue.permission_id,
            pool.clone(),
        )
        .await?;

        let sign_num: i64 = signature
            .iter()
            .filter_map(|sig| if sig.singed == 1 { Some(1) } else { None })
            .sum();

        let extra = MultisigQueueDomain::handle_queue_extra(&queue, &pool).await?;

        Ok(MultisigQueueInfoVo {
            queue,
            signature,
            sign_num,
            extra: extra.unwrap_or_default(),
        })
    }

    // only solana used
    pub async fn sign_fee(
        queue_id: String,
        address: String,
    ) -> Result<response_vo::EstimateFeeResp, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let queue = MultisigDomain::queue_by_id(&queue_id, &pool).await?;
        let multisig_account =
            MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;

        let adapter = ChainAdapterFactory::get_multisig_adapter(&queue.chain_code).await?;

        let main_coin = ChainTransaction::main_coin(&queue.chain_code).await?;

        let res = adapter
            .sign_fee(
                &multisig_account,
                &address,
                &queue.raw_data,
                &main_coin.symbol,
            )
            .await?;

        let fee_resp =
            response_vo::EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), res);
        Ok(fee_resp)
    }

    // multisig sign message
    pub async fn sign_multisig_transaction(
        queue_id: &str,
        status: i32,
        password: &str,
        address: Option<String>,
    ) -> Result<(), crate::ServiceError> {
        let status = MultisigSignatureStatus::try_from(status)
            .map_err(|e| crate::ServiceError::Parameter(e.to_string()))?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let queue = MultisigDomain::queue_by_id(queue_id, &pool).await?;
        MultisigQueueDomain::validate_queue(&queue, false)?;

        let sign_addr = Self::get_address_to_sign(pool.clone(), &queue, address).await?;
        // 1.签名
        let signed =
            Self::_sign_transaction(&queue, status, pool.clone(), password, sign_addr).await?;

        // 2.同步签名的结果状态
        MultisigQueueRepo::sync_sign_status(&queue, queue.status, pool.clone()).await?;

        // 3. 签名的结果发送给后端
        MultisigQueueDomain::upload_queue_sign(queue_id, pool, signed, status).await
    }

    // 查找需要进行签名的地址
    pub async fn get_address_to_sign(
        pool: DbPool,
        queue: &MultisigQueueEntity,
        address: Option<String>,
    ) -> Result<Vec<String>, crate::ServiceError> {
        match address {
            Some(address) => Ok(vec![address]),
            None => {
                // 区分是多签还是普通权限
                if !queue.account_id.is_empty() {
                    let member =
                        MultisigQueueRepo::self_member_by_account(&queue.account_id, &pool).await?;

                    Ok(member.get_owner_str_vec())
                } else {
                    let users = PermissionRepo::self_user(&pool, &queue.permission_id).await?;

                    Ok(users.iter().map(|u| u.address.clone()).collect())
                }
            }
        }
    }

    pub async fn multisig_transfer_fee(
        queue_id: &str,
    ) -> Result<response_vo::EstimateFeeResp, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let queue = MultisigDomain::queue_by_id(queue_id, &pool).await?;

        let assets = domain::chain::transaction::ChainTransaction::assets(
            &queue.chain_code,
            &queue.symbol,
            &queue.from_addr,
        )
        .await?;

        // 签名数
        let signs = MultisigQueueRepo::get_signed_list(&pool, queue_id).await?;
        let sign_list = signs.get_order_sign_str();

        let instance = ChainAdapterFactory::get_multisig_adapter(&queue.chain_code).await?;
        let main_coin = ChainTransaction::main_coin(&assets.chain_code).await?;

        let backend = crate::manager::Context::get_global_backend_api()?;
        let fee = instance
            .estimate_fee(
                &queue,
                &assets,
                backend,
                sign_list,
                main_coin.symbol.as_str(),
            )
            .await?;

        let fee_resp =
            response_vo::EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), fee);
        Ok(fee_resp)
    }

    // 执行签名
    pub async fn _sign_transaction(
        queue: &MultisigQueueEntity,
        status: MultisigSignatureStatus,
        pool: DbPool,
        password: &str,
        sign_addr: Vec<String>,
    ) -> Result<Vec<NewSignatureEntity>, crate::ServiceError> {
        match status {
            MultisigSignatureStatus::Rejected | MultisigSignatureStatus::UnSigned => {
                let mut result = vec![];
                for address in sign_addr {
                    let params = NewSignatureEntity::new(&queue.id, &address, "", status);

                    MultisigQueueRepo::create_or_update_sign(&params, &pool).await?;

                    result.push(params);
                }
                Ok(result)
            }
            MultisigSignatureStatus::Approved => {
                if !queue.account_id.is_empty() {
                    Self::signed_account(queue, pool, sign_addr, password, status).await
                } else {
                    Self::signed_permission(queue, pool, sign_addr, password, status).await
                }
            }
        }
    }

    async fn signed_account(
        queue: &MultisigQueueEntity,
        pool: DbPool,
        sign_addr: Vec<String>,
        password: &str,
        status: MultisigSignatureStatus,
    ) -> Result<Vec<NewSignatureEntity>, crate::ServiceError> {
        let mut result = vec![];

        let multisig_account =
            MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;
        // 当前已签名的数量
        let sign_list = MultisigQueueRepo::get_signed_list(&pool, &queue.id).await?;

        let need_sign = sign_list.need_signed_num(multisig_account.threshold as usize);
        if need_sign == 0 {
            return Ok(result);
        }

        let mut signed_num = 0;

        // 批量执行签名
        let instance = ChainAdapterFactory::get_multisig_adapter(&queue.chain_code).await?;
        for i in 0..sign_addr.len() {
            let address = sign_addr.get(i).unwrap();
            // filter already signed
            if sign_list.contains_address(address) {
                continue;
            };

            let key =
                ChainTransaction::get_key(&address, &queue.chain_code, password, &None).await?;

            let rs = instance
                .sign_multisig_tx(&multisig_account, address, key, &queue.raw_data)
                .await?;
            let params = NewSignatureEntity::new(&queue.id, address, &rs.signature, status);

            MultisigQueueRepo::create_or_update_sign(&params, &pool).await?;
            result.push(params);

            if queue.chain_code == chain_code::SOLANA {
                let tx = NewBillEntity::new_signed_bill(
                    rs.tx_hash,
                    address.clone(),
                    queue.chain_code.clone(),
                );
                domain::bill::BillDomain::create_bill(tx).await?;
            }

            signed_num += 1;
            if signed_num >= need_sign {
                break;
            }
        }

        Ok(result)
    }

    // 目前只有tron链实现了权限相关的内容
    async fn signed_permission(
        queue: &MultisigQueueEntity,
        pool: DbPool,
        sign_addr: Vec<String>,
        password: &str,
        status: MultisigSignatureStatus,
    ) -> Result<Vec<NewSignatureEntity>, crate::ServiceError> {
        let mut result = vec![];

        let permission = PermissionRepo::find_by_id(&pool, &queue.permission_id).await?;

        // 当前已签名的数量
        let sign_list = MultisigQueueRepo::signed_result(
            &queue.id,
            &queue.account_id,
            &queue.permission_id,
            pool.clone(),
        )
        .await?;

        // 需要签名的阈值
        let total_weight = sign_list.iter().map(|s| s.weight).sum::<i64>();
        let need_sign = (permission.threshold - total_weight).max(0);
        if need_sign == 0 {
            return Ok(result);
        }

        // let mut signed_num = 0;

        // 批量执行签名
        for i in 0..sign_addr.len() {
            let address = sign_addr.get(i).unwrap();
            // filter already signed
            if sign_list
                .iter()
                .find(|item| item.address == *address && !item.signature.is_empty())
                .is_some()
            {
                continue;
            }

            let key =
                ChainTransaction::get_key(&address, &queue.chain_code, password, &None).await?;

            let res =
                tron::operations::multisig::TransactionOpt::sign_transaction(&queue.raw_data, key)?;
            let params = NewSignatureEntity::new(&queue.id, address, &res.signature, status);

            MultisigQueueRepo::create_or_update_sign(&params, &pool).await?;
            result.push(params);

            if queue.chain_code == chain_code::SOLANA {
                let tx = NewBillEntity::new_signed_bill(
                    res.tx_hash,
                    address.clone(),
                    queue.chain_code.clone(),
                );
                domain::bill::BillDomain::create_bill(tx).await?;
            }

            // signed_num += 1;
            // if signed_num >= need_sign {
            //     break;
            // }
        }

        Ok(result)
    }

    pub async fn exec_multisig_transaction(
        queue_id: &str,
        password: String,
        fee_setting: Option<String>,
        request_resource_id: Option<String>,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let queue = MultisigDomain::queue_by_id(queue_id, &pool).await?;
        MultisigQueueDomain::validate_queue(&queue, true)?;

        let mut repo = MultisigQueueRepo::new(pool.clone());

        let signs = MultisigQueueRepo::get_signed_list(&pool, queue_id).await?;
        let signs_list = signs.get_order_sign_str();

        let assets =
            ChainTransaction::assets(&queue.chain_code, &queue.symbol, &queue.from_addr).await?;
        let transfer_amount = wallet_utils::unit::convert_to_u256(&queue.value, assets.decimals)?;

        let bill_kind = BillKind::try_from(queue.transfer_type)?;

        let instance = ChainAdapterFactory::get_multisig_adapter(&queue.chain_code).await?;
        let tx_resp = match instance {
            MultisigAdapter::Ethereum(chain) => {
                let multisig_account =
                    MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;

                let signatures = signs_list.join("");
                let params = eth::operations::MultisigTransferOpt::new(
                    &queue.from_addr,
                    &queue.to_addr,
                    alloy::primitives::U256::default(),
                )?
                .exec_params(
                    &multisig_account.initiator_addr,
                    queue.raw_data,
                    signatures,
                )?;

                let key = crate::domain::account::open_subpk_with_password(
                    &queue.chain_code,
                    &multisig_account.initiator_addr,
                    &password,
                )
                .await?;

                let fee_setting = if let Some(fee_setting) = fee_setting {
                    crate::domain::chain::pare_fee_setting(&fee_setting)?
                } else {
                    return Err(crate::ServiceError::Parameter(
                        "empty fee setting".to_string(),
                    ));
                };

                // check transaction fee
                let balance = chain
                    .balance(&multisig_account.initiator_addr, None)
                    .await?;

                let fee = fee_setting.transaction_fee();
                if balance < fee {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientFeeBalance,
                    ))?;
                }

                let tx_hash = chain.exec_transaction(params, fee_setting, key).await?;
                TransferResp::new(
                    tx_hash,
                    unit::format_to_string(fee, eth::consts::ETH_DECIMAL)?,
                )
            }
            MultisigAdapter::BitCoin(chain) => {
                let account =
                    MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;

                // 如果是p2tr-sh地址类型需要单独处理签名顺序问题
                let sign = if account.address_type == "p2tr-sh" {
                    let member =
                        MultisigMemberDaoV1::find_records_by_id(&account.id, pool.as_ref())
                            .await
                            .map_err(|e| crate::ServiceError::Database(e.into()))?;
                    member.sign_order(&signs.0)
                } else {
                    signs_list.clone()
                };

                let params = btc::operations::multisig::MultisigTransactionOpt::new(
                    account.address,
                    queue.value.clone(),
                    &account.salt,
                    &queue.raw_data,
                    &account.address_type,
                )?;

                let tx = chain
                    .exec_multisig_tx(params, sign, account.authority_addr)
                    .await
                    .map_err(domain::chain::transaction::ChainTransaction::handle_btc_fee_error)?;

                TransferResp::new(tx.tx_hash, tx.fee.to_string())
            }
            MultisigAdapter::Solana(chain) => {
                let multisig_account =
                    MultisigDomain::account_by_address(&queue.from_addr, true, &pool).await?;

                let key = crate::domain::account::open_subpk_with_password(
                    &queue.chain_code,
                    &multisig_account.initiator_addr,
                    &password,
                )
                .await?;

                let params = sol::operations::multisig::transfer::ExecMultisigOpt::new(
                    &multisig_account.initiator_addr,
                    queue.raw_data,
                )?;

                let instructions = params.instructions().await?;

                // check transaction_fee
                let mut fee = chain.estimate_fee_v1(&instructions, &params).await?;
                ChainTransaction::sol_priority_fee(&mut fee, queue.token_addr.as_ref(), 200_000);

                let balance = chain
                    .balance(&multisig_account.initiator_addr, None)
                    .await?;
                ChainTransaction::check_sol_transaction_fee(balance, fee.original_fee())?;

                let fees = fee.transaction_fee().to_string();

                let tx_hash = chain
                    .exec_transaction(params, key, Some(fee), instructions, 0)
                    .await?;

                TransferResp::new(tx_hash, fees)
            }
            MultisigAdapter::Tron(chain) => {
                // check balance
                let params =
                    tron::operations::multisig::TransactionOpt::data_from_str(&queue.raw_data)?;
                let provider = chain.get_provider();

                let transfer_balance = chain
                    .balance(&queue.from_addr, queue.token_address())
                    .await?;

                // 根据交易类型来判断是否需要将amount 进行验证
                let transfer_amount = if bill_kind.out_transfer_type() {
                    transfer_amount
                } else {
                    alloy::primitives::U256::ZERO
                };
                if transfer_balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }

                let account = provider.account_info(&queue.from_addr).await?;
                let consumer = if let Some(token) = queue.token_address() {
                    let assets = domain::chain::transaction::ChainTransaction::assets(
                        &queue.chain_code,
                        &queue.symbol,
                        &queue.from_addr,
                    )
                    .await?;

                    let memo = (!queue.notes.is_empty()).then(|| queue.notes.clone());

                    let value = unit::convert_to_u256(&queue.value, assets.decimals)?;
                    let transfer_params = tron::operations::transfer::ContractTransferOpt::new(
                        &token,
                        &queue.from_addr,
                        &queue.to_addr,
                        value,
                        memo,
                    )?;
                    let constant = transfer_params.constant_contract(provider).await?;

                    let consumer = provider
                        .contract_fee(
                            constant,
                            signs_list.len() as u8,
                            &transfer_params.owner_address,
                        )
                        .await?;
                    // check transaction fee
                    if account.balance < consumer.transaction_fee_i64() {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }

                    consumer
                } else {
                    let to = (!queue.to_addr.is_empty()).then_some(queue.to_addr.as_str());

                    let mut consumer = provider
                        .transfer_fee(
                            &queue.from_addr,
                            to,
                            &params.raw_data_hex,
                            signs_list.len() as u8,
                        )
                        .await?;

                    if queue.transfer_type == BillKind::UpdatePermission.to_i8() {
                        consumer.set_extra_fee(100 * tron::consts::TRX_VALUE);
                    }

                    let value = transfer_amount.to::<i64>();
                    if account.balance < consumer.transaction_fee_i64() + value {
                        return Err(crate::BusinessError::Chain(
                            crate::ChainError::InsufficientFeeBalance,
                        ))?;
                    }

                    consumer
                };

                let bill_consumer = BillResourceConsume::new_tron(
                    consumer.bandwidth.consumer as u64,
                    consumer.get_energy(),
                );
                let tx_hash = chain.exec_multisig_transaction(params, signs_list).await?;

                let mut resp = TransferResp::new(tx_hash, consumer.transaction_fee());
                resp.with_consumer(bill_consumer);
                resp
            }
        };

        // 创建本地pending 交易
        let tx = NewBillEntity::new(
            tx_resp.tx_hash.clone(),
            queue.from_addr,
            queue.to_addr,
            queue.value.parse().unwrap_or_default(),
            queue.chain_code,
            queue.symbol,
            true,
            bill_kind,
            queue.notes.clone(),
        )
        .with_queue_id(queue_id)
        .with_resource_consume(&tx_resp.resource_consume()?)
        .with_transaction_fee(&tx_resp.fee);

        domain::bill::BillDomain::create_bill(tx).await?;

        // sync status and tx_hash
        repo.update_status_and_hash(
            queue_id,
            MultisigQueueStatus::InConfirmation,
            &tx_resp.tx_hash,
        )
        .await?;

        let raw_data = MultisigQueueRepo::multisig_queue_data(queue_id, pool)
            .await?
            .to_string()?;
        // 上报后端
        let req = SignedTranUpdateHashReq {
            withdraw_id: queue_id.to_string(),
            hash: tx_resp.tx_hash.clone(),
            remark: queue.notes,
            raw_data,
        };

        let backend = crate::manager::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        if let Err(e) = backend.signed_tran_update_trans_hash(cryptor, &req).await {
            tracing::error!("report signed tran update  add to task{}", e);
            let task = Task::BackendApi(BackendApiTask::BackendApi(BackendApiTaskData {
                endpoint: endpoint::multisig::SIGNED_TRAN_UPDATE_TRANS_HASH.to_string(),
                body: serde_func::serde_to_value(&req)?,
            }));
            Tasks::new().push(task).send().await?;
        }

        // 回收资源
        if let Some(request_id) = request_resource_id {
            let _rs = backend.delegate_complete(cryptor, &request_id).await;
        }

        Ok(tx_resp.tx_hash)
    }

    pub async fn check_ongoing_queue(
        chain_code: String,
        address: String,
    ) -> Result<Option<QueueInfo>, crate::ServiceError> {
        if chain_code.as_str() == chain_code::ETHEREUM
            || chain_code.as_str() == chain_code::BNB
            || chain_code.as_str() == chain_code::BTC
        {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;

            let rs = MultisigQueueRepo::ongoing_queue(&chain_code, &address, &pool)
                .await?
                .map(QueueInfo::from);
            Ok(rs)
        } else {
            Ok(None)
        }
    }

    // cancel multisig queue
    pub async fn cancel_queue(queue_id: String) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let queue = MultisigDomain::queue_by_id(&queue_id, &pool).await?;

        // check status
        if !queue.can_cancel() {
            return Err(crate::BusinessError::MultisigQueue(
                crate::MultisigQueueError::CannotCancel,
            ))?;
        };

        // update status to fail
        MultisigQueueRepo::update_fail(&pool, &queue_id, fail_reason::CANCEL).await?;

        // report to backend ,if error rollback status
        let raw_data = MultisigQueueRepo::multisig_queue_data(&queue_id, pool.clone())
            .await?
            .to_string()?;
        let backend = crate::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        if let Err(e) = backend
            .signed_trans_cancel(cryptor, &queue_id, raw_data)
            .await
        {
            tracing::error!("cancel queue[{}] upload fail roolback err:{}", queue_id, e);
            MultisigQueueDaoV1::rollback_update_fail(&queue_id, queue.status, pool.as_ref())
                .await
                .map_err(|e| crate::ServiceError::Database(wallet_database::Error::Database(e)))?;
        }

        Ok(())
    }
}
