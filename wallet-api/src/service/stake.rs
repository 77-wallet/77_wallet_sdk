use crate::domain;
use crate::domain::chain::adapter::ChainAdapterFactory;
use crate::domain::chain::transaction::ChainTransDomain;
use crate::domain::coin::TokenCurrencyGetter;
use crate::domain::multisig::MultisigDomain;
use crate::domain::multisig::MultisigQueueDomain;
use crate::domain::stake::EstimateTxConsumer;
use crate::domain::stake::StakeArgs;
use crate::domain::stake::StakeDomain;
use crate::error::business::stake::StakeError;
use crate::infrastructure::task_queue;
use crate::infrastructure::task_queue::BackendApiTaskData;
use crate::manager::Context;
use crate::messaging::notify::event::NotifyEvent;
use crate::messaging::notify::other::Process;
use crate::messaging::notify::other::TransactionProcessFrontend;
use crate::messaging::notify::FrontendNotifyEvent;
use crate::request::stake;
use crate::request::transaction::Signer;
use crate::response_vo::account::AccountResource;
use crate::response_vo::account::BalanceInfo;
use crate::response_vo::account::Resource;
use crate::response_vo::account::TrxResource;
use crate::response_vo::stake as resp;
use crate::response_vo::stake::AddressExists;
use crate::response_vo::stake::BatchDelegateResp;
use crate::response_vo::stake::BatchRes;
use crate::response_vo::stake::DelegateListResp;
use crate::response_vo::stake::DelegateRemaingTime;
use crate::response_vo::stake::ResourceResp;
use crate::response_vo::EstimateFeeResp;
use crate::response_vo::TronFeeDetails;
use crate::BusinessError;
use wallet_chain_interact::tron;
use wallet_chain_interact::tron::operations as ops;
use wallet_chain_interact::tron::operations::stake::DelegateArgs;
use wallet_chain_interact::tron::operations::stake::ResourceType;
use wallet_chain_interact::tron::operations::stake::UnDelegateArgs;
use wallet_chain_interact::tron::operations::RawTransactionParams;
use wallet_chain_interact::tron::params::ResourceConsumer;
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;
use wallet_chain_interact::tron::protocol::account::TronAccount;
use wallet_chain_interact::tron::protocol::ChainParameter;
use wallet_chain_interact::tron::TronChain;
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_chain_interact::BillResourceConsume;
use wallet_database::dao::bill::BillDao;
use wallet_database::entities::bill::BillKind;
use wallet_database::entities::bill::NewBillEntity;
use wallet_database::entities::multisig_queue::NewMultisigQueueEntity;
use wallet_database::pagination::Pagination;
use wallet_database::repositories::multisig_queue::MultisigQueueRepo;
use wallet_database::repositories::permission::PermissionRepo;
use wallet_transport_backend::api::permission::TransPermission;
use wallet_transport_backend::consts::endpoint;
use wallet_transport_backend::request::PermissionData;
use wallet_transport_backend::response_vo::stake::SystemEnergyResp;
use wallet_types::constant::chain_code;
use wallet_utils::serde_func;

struct TempBuildTransaction {
    to: String,
    value: f64,
    conusmer: tron::params::ResourceConsumer,
    raw_data: RawTransactionParams,
}

pub struct StackService {
    chain: TronChain,
}

impl StackService {
    pub async fn new() -> Result<Self, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        Ok(Self { chain })
    }

    // 获取私钥
    pub async fn get_key(
        &self,
        from: &str,
        signer: &Option<Signer>,
        password: &str,
    ) -> Result<wallet_chain_interact::types::ChainPrivateKey, crate::ServiceError> {
        ChainTransDomain::get_key(from, chain_code::TRON, password, signer).await
    }

    async fn process_transaction<T>(
        &self,
        args: impl ops::TronTxOperation<T>,
        bill_kind: BillKind,
        from: &str,
        value: i64,
        bill_value: f64,
        signer: &Option<Signer>,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        // 构建交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let key = self.get_key(&from, signer, password).await?;

        let resp = args.build_raw_transaction(&self.chain.provider).await?;
        // 验证余额
        let balance = self.chain.balance(from, None).await?;
        let consumer = self
            .chain
            .get_provider()
            .transfer_fee(from, None, &resp.raw_data_hex, 1)
            .await?;

        if balance.to::<i64>() < consumer.transaction_fee_i64() + value * tron::consts::TRX_VALUE {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientFeeBalance,
            ))?;
        }

        // 广播交易交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let hash = self.chain.exec_transaction_v1(resp, key).await?;

        let transaction_fee = consumer.transaction_fee();

        // 写入本地交易数据
        let value = if bill_value > 0.0 {
            bill_value
        } else {
            args.get_value()
        };
        let bill_consumer = BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0);
        let mut entity = NewBillEntity::new_stake_bill(
            hash.clone(),
            from.to_string(),
            args.get_to(),
            value,
            bill_kind,
            bill_consumer.to_json_str()?,
            transaction_fee,
        );

        // if use permission upload backend
        if let Some(signer) = signer {
            let pool = crate::Context::get_global_sqlite_pool()?;
            let permission =
                PermissionRepo::permission_with_user(&pool, from, signer.permission_id, false)
                    .await?
                    .ok_or(crate::BusinessError::Permission(
                        crate::PermissionError::ActivesPermissionNotFound,
                    ))?;

            let users = permission.users();
            let params = TransPermission {
                address: from.to_string(),
                chain_code: chain_code::TRON.to_string(),
                tx_kind: bill_kind.to_i8(),
                hash: hash.clone(),
                permission_data: PermissionData {
                    opt_address: signer.address.to_string(),
                    users: users.clone(),
                },
            };

            let task = task_queue::Task::BackendApi(task_queue::BackendApiTask::BackendApi(
                BackendApiTaskData::new(endpoint::UPLOAD_PERMISSION_TRANS, &params)?,
            ));
            let _ = task_queue::Tasks::new().push(task).send().await;
            entity.signer = users;
        }

        domain::bill::BillDomain::create_bill(entity).await?;

        Ok(hash)
    }

    // 批量构建交易并发送通知到前端(return:raw_transaction and to_address)
    async fn batch_build_transaction<T>(
        &self,
        args: Vec<impl ops::TronTxOperation<T>>,
        bill_kind: BillKind,
        chain_param: &ChainParameter,
        resource: &AccountResourceDetail,
    ) -> Result<Vec<TempBuildTransaction>, crate::ServiceError> {
        let mut result = vec![];

        for (i, item) in args.into_iter().enumerate() {
            let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new_with_num(
                bill_kind,
                (i + 1) as i64,
                Process::Building,
            ));
            FrontendNotifyEvent::new(data).send().await?;

            let res = item.build_raw_transaction(&self.chain.provider).await?;

            // 计算费用
            let bandwidth = self.chain.provider.calc_bandwidth(&res.raw_data_hex, 1);
            let bandwidth = tron::params::Resource::new(
                resource.available_bandwidth(),
                bandwidth,
                chain_param.get_transaction_fee(),
                "bandwidth",
            );

            let temp = TempBuildTransaction {
                raw_data: res,
                to: item.get_to(),
                value: item.get_value(),
                conusmer: ResourceConsumer::new(bandwidth, None),
            };

            result.push(temp);
        }

        Ok(result)
    }

    async fn batch_exec(
        &self,
        from: &str,
        key: ChainPrivateKey,
        bill_kind: BillKind,
        txs: Vec<TempBuildTransaction>,
    ) -> Result<(Vec<BatchRes>, Vec<String>), crate::ServiceError> {
        let mut exce_res = vec![];
        let mut tx_hash = vec![];

        for (i, item) in txs.into_iter().enumerate() {
            let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new_with_num(
                bill_kind,
                (i + 1) as i64,
                Process::Broadcast,
            ));
            FrontendNotifyEvent::new(data).send().await?;

            let result = self
                .chain
                .exec_transaction_v1(item.raw_data, key.clone())
                .await;
            match result {
                Ok(hash) => {
                    let transaction_fee = item.conusmer.transaction_fee();

                    let bill_consumer =
                        BillResourceConsume::new_tron(item.conusmer.act_bandwidth() as u64, 0)
                            .to_json_str()?;
                    let entity = NewBillEntity::new_stake_bill(
                        hash.clone(),
                        from.to_string(),
                        item.to.clone(),
                        item.value,
                        bill_kind,
                        bill_consumer,
                        transaction_fee,
                    );
                    domain::bill::BillDomain::create_bill(entity).await?;

                    let ra = BatchRes {
                        address: item.to.clone(),
                        status: true,
                    };
                    exce_res.push(ra);
                    tx_hash.push(hash);
                }
                Err(_e) => {
                    let ra = BatchRes {
                        address: item.to,
                        status: false,
                    };
                    exce_res.push(ra);
                }
            }
        }
        Ok((exce_res, tx_hash))
    }

    async fn resource_value(
        &self,
        owner_address: &str,
        value: i64,
        resource_type: ops::stake::ResourceType,
    ) -> Result<f64, crate::ServiceError> {
        let resource = ChainTransDomain::account_resource(&self.chain, owner_address).await?;
        Ok(resource.resource_value(resource_type, value)?)
    }

    async fn process_delegate(
        &self,
        chain: &TronChain,
        resource_type: &Option<String>,
        owner_address: &str,
        to: &str,
        resource: &AccountResourceDetail,
    ) -> Result<Vec<resp::DelegateListResp>, crate::ServiceError> {
        let mut result = Vec::new();

        let res = chain.provider.delegated_resource(owner_address, to).await?;

        for delegate in res.delegated_resource {
            if let Some(types) = &resource_type {
                if types.eq_ignore_ascii_case("bandwidth")
                    && delegate.frozen_balance_for_bandwidth > 0
                {
                    let resource_type = ops::stake::ResourceType::BANDWIDTH;
                    let amount = delegate.value_trx(resource_type);
                    let resource_value = resource.resource_value(resource_type, amount)?;

                    let resource = resp::ResourceResp::new(amount, resource_type, resource_value);
                    let r = resp::DelegateListResp::new(
                        &delegate,
                        resource,
                        delegate.expire_time_for_bandwidth,
                    )?;
                    result.push(r);
                }

                if types.eq_ignore_ascii_case("energy") && delegate.frozen_balance_for_energy > 0 {
                    let resource_type = ops::stake::ResourceType::ENERGY;
                    let amount = delegate.value_trx(resource_type);
                    let resource_value = resource.resource_value(resource_type, amount)?;

                    let resource = resp::ResourceResp::new(amount, resource_type, resource_value);
                    let r = resp::DelegateListResp::new(
                        &delegate,
                        resource,
                        delegate.expire_time_for_energy,
                    )?;
                    result.push(r);
                }
            } else {
                // 处理能源类型
                if delegate.frozen_balance_for_energy > 0 {
                    let resource_type = ops::stake::ResourceType::ENERGY;
                    let amount = delegate.value_trx(resource_type);
                    let resource_value = resource.resource_value(resource_type, amount)?;

                    let resource = resp::ResourceResp::new(amount, resource_type, resource_value);
                    let r = resp::DelegateListResp::new(
                        &delegate,
                        resource,
                        delegate.expire_time_for_energy,
                    )?;
                    result.push(r);
                }

                // 处理带宽类型
                if delegate.frozen_balance_for_bandwidth > 0 {
                    let resource_type = ops::stake::ResourceType::BANDWIDTH;
                    let amount = delegate.value_trx(resource_type);
                    let resource_value = resource.resource_value(resource_type, amount)?;

                    let resource = resp::ResourceResp::new(amount, resource_type, resource_value);
                    let r = resp::DelegateListResp::new(
                        &delegate,
                        resource,
                        delegate.expire_time_for_bandwidth,
                    )?;
                    result.push(r);
                }
            }
        }

        Ok(result)
    }

    fn fill_resource_info(
        &self,
        resource: &mut Resource,
        account: &TronAccount,
        resource_type: ops::stake::ResourceType,
        price: f64,
    ) {
        // 代理给别人的
        let delegate = account.delegate_resource(resource_type);
        resource.delegate_freeze = TrxResource::new(delegate, price);

        // 别人我给代理的
        let acquire = account.acquired_resource(resource_type);
        resource.acquire_freeze = TrxResource::new(acquire, price);

        // 自己质押的
        let type_str = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => "",
            ops::stake::ResourceType::ENERGY => "ENERGY",
        };
        let owner = account.frozen_v2_owner(type_str);
        resource.owner_freeze = TrxResource::new(owner + delegate, price);

        resource.can_unfreeze = owner;

        // 解冻中的金额
        let un_freeze = account.un_freeze_amount(type_str);
        resource.un_freeze = TrxResource::new(un_freeze, price);

        // 可提取的
        let can_withdraw = account.can_withdraw_unfreeze_amount(type_str);
        resource.pending_withdraw = TrxResource::new(can_withdraw, price);

        // 价格
        resource.price = price;

        // 计算可转账次数
        resource.calculate_transfer_times();

        // 计算总的质押
        resource.calculate_total();
    }

    async fn convert_stake_args(
        &self,
        bill_kind: BillKind,
        content: String,
    ) -> Result<(StakeArgs, String, f64, Option<Signer>), crate::ServiceError> {
        match bill_kind {
            BillKind::FreezeBandwidth | BillKind::FreezeEnergy => {
                let req = serde_func::serde_from_str::<stake::FreezeBalanceReq>(&content)?;
                let args = ops::stake::FreezeBalanceArgs::try_from(&req)?;

                Ok((
                    StakeArgs::Freeze(args),
                    req.owner_address.clone(),
                    req.frozen_balance as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::UnFreezeBandwidth | BillKind::UnFreezeEnergy => {
                let req = serde_func::serde_from_str::<stake::UnFreezeBalanceReq>(&content)?;
                let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

                Ok((
                    StakeArgs::UnFreeze(args),
                    req.owner_address.clone(),
                    req.unfreeze_balance as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::CancelAllUnFreeze => {
                let req = serde_func::serde_from_str::<stake::CancelAllUnFreezeReq>(&content)?;

                let account = self.chain.account_info(&req.owner_address).await?;
                let bandwidth = account.un_freeze_amount("");
                let energy = account.un_freeze_amount("ENERGY");

                let args = ops::stake::CancelAllFreezeBalanceArgs::new(
                    &req.owner_address,
                    req.signer.clone().map(|s| s.permission_id),
                )?;

                Ok((
                    StakeArgs::CancelAllUnFreeze(args),
                    req.owner_address.clone(),
                    (bandwidth + energy) as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::WithdrawUnFreeze => {
                let req = serde_func::serde_from_str::<stake::WithdrawBalanceReq>(&content)?;

                let can_widthdraw = self
                    .chain
                    .provider
                    .can_withdraw_unfreeze_amount(&req.owner_address)
                    .await?;
                let args = ops::stake::WithdrawUnfreezeArgs {
                    owner_address: req.owner_address.clone(),
                    permission_id: req.signer.clone().map(|s| s.permission_id),
                };

                Ok((
                    StakeArgs::Withdraw(args),
                    req.owner_address.clone(),
                    can_widthdraw.to_sun() as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::DelegateBandwidth | BillKind::DelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::DelegateReq>(&content)?;
                let args = ops::stake::DelegateArgs::try_from(&req)?;

                Ok((
                    StakeArgs::Delegate(args),
                    req.owner_address.clone(),
                    req.balance as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::UnDelegateBandwidth | BillKind::UnDelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::UnDelegateReq>(&content)?;
                let args = ops::stake::UnDelegateArgs::try_from(&req)?;

                Ok((
                    StakeArgs::UnDelegate(args),
                    req.owner_address.clone(),
                    req.balance as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::BatchDelegateBandwidth | BillKind::BatchDelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::BatchDelegate>(&content)?;

                let value = req.list.iter().map(|item| item.value).sum::<i64>() as f64;

                let args: Vec<DelegateArgs> = (&req).try_into()?;
                Ok((
                    StakeArgs::BatchDelegate(args),
                    req.owner_address.clone(),
                    value,
                    req.signer.clone(),
                ))
            }
            BillKind::BatchUnDelegateBandwidth | BillKind::BatchUnDelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::BatchUnDelegate>(&content)?;

                let value = req.list.iter().map(|item| item.value).sum::<i64>() as f64;
                let args: Vec<UnDelegateArgs> = (&req).try_into()?;
                Ok((
                    StakeArgs::BatchUnDelegate(args),
                    req.owner_address.clone(),
                    value,
                    req.signer.clone(),
                ))
            }
            BillKind::Vote => {
                let req = serde_func::serde_from_str::<stake::VoteWitnessReq>(&content)?;

                let args = ops::stake::VoteWitnessArgs::try_from(&req)?;
                Ok((
                    StakeArgs::Votes(args),
                    req.owner_address.clone(),
                    req.get_votes() as f64,
                    req.signer.clone(),
                ))
            }
            BillKind::WithdrawReward => {
                let req = serde_func::serde_from_str::<stake::WithdrawBalanceReq>(&content)?;

                let value = self
                    .chain
                    .get_provider()
                    .get_reward(&req.owner_address)
                    .await?
                    .to_sun();

                let args = ops::stake::WithdrawBalanceArgs::try_from(&req)?;

                Ok((
                    StakeArgs::WithdrawReward(args),
                    req.owner_address.clone(),
                    value,
                    req.signer.clone(),
                ))
            }
            _ => Err(crate::BusinessError::Stake(
                crate::StakeError::UnSupportBillKind,
            ))?,
        }
    }
}

impl StackService {
    pub async fn estimate_stake_fee(
        &self,
        bill_kind: i64,
        content: String,
    ) -> Result<EstimateFeeResp, crate::ServiceError> {
        let bill_kind = BillKind::try_from(bill_kind as i8)?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();
        let token_currency = TokenCurrencyGetter::get_currency(currency, "tron", "TRX").await?;

        let (args, account, _, _) = self.convert_stake_args(bill_kind, content).await?;

        let consumer = args.exec(&account, &self.chain).await?;
        let res = TronFeeDetails::new(consumer, token_currency, currency)?;

        let content = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok(EstimateFeeResp::new(
            "TRX".to_string(),
            chain_code::TRON.to_string(),
            content,
        ))
    }

    // 质押trx
    pub async fn freeze_balance(
        &self,
        req: stake::FreezeBalanceReq,
        password: &str,
    ) -> Result<resp::FreezeResp, crate::ServiceError> {
        let from = req.owner_address.clone();
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let args = ops::stake::FreezeBalanceArgs::try_from(&req)?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::FreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::FreezeEnergy,
        };

        let tx_hash = self
            .process_transaction(
                args,
                bill_kind,
                &from,
                req.frozen_balance,
                0.0,
                &req.signer,
                password,
            )
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.frozen_balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.frozen_balance, resource_type, resource_value);
        Ok(resp::FreezeResp::new(
            req.owner_address.clone(),
            resource,
            tx_hash,
            bill_kind,
        ))
    }

    // 质押明细
    pub async fn freeze_list(
        &self,
        owner: &str,
    ) -> Result<Vec<resp::FreezeListResp>, crate::error::ServiceError> {
        let account = self.chain.account_info(owner).await?;
        let resource = self.chain.account_resource(owner).await?;

        let pool = crate::Context::get_global_sqlite_pool()?;

        let mut res = vec![];

        // 包含（质押中、解锁中、代理给别人的）
        let mut bandwidth = account.frozen_v2_owner("");
        bandwidth += account.delegate_resource(ops::stake::ResourceType::BANDWIDTH);

        if bandwidth > 0 {
            let resource_type = ops::stake::ResourceType::BANDWIDTH;
            let resource_value = resource.resource_value(resource_type, bandwidth)?;

            let resource_resp = resp::ResourceResp::new(bandwidth, resource_type, resource_value);
            let mut freeze = resp::FreezeListResp::new(resource_resp);

            let kinds = vec![
                BillKind::FreezeBandwidth.to_i8(),
                BillKind::UnFreezeBandwidth.to_i8(),
            ];
            let bill = BillDao::last_kind_bill(pool.as_ref(), owner, kinds).await?;
            if let Some(bill) = bill {
                freeze.opration_time = Some(bill.transaction_time)
            }

            res.push(freeze);
        }

        let mut energy = account.frozen_v2_owner("ENERGY");
        energy += account.delegate_resource(ops::stake::ResourceType::ENERGY);

        if energy > 0 {
            let resource_type = ops::stake::ResourceType::ENERGY;
            let resource_value = resource.resource_value(resource_type, energy)?;

            let resource_resp = resp::ResourceResp::new(energy, resource_type, resource_value);
            let mut freeze = resp::FreezeListResp::new(resource_resp);

            let kinds = vec![
                BillKind::FreezeEnergy.to_i8(),
                BillKind::UnFreezeEnergy.to_i8(),
            ];
            let bill = BillDao::last_kind_bill(pool.as_ref(), owner, kinds).await?;
            if let Some(bill) = bill {
                freeze.opration_time = Some(bill.transaction_time)
            }

            res.push(freeze);
        }

        Ok(res)
    }

    pub async fn un_freeze_list(
        &self,
        owner: &str,
    ) -> Result<Vec<resp::UnfreezeListResp>, crate::error::ServiceError> {
        let account = self.chain.account_info(owner).await?;

        let now = wallet_utils::time::now().timestamp_millis();

        let mut result = account
            .unfreeze_v2
            .iter()
            .filter(|item| item.unfreeze_expire_time > now)
            .map(|item| {
                let resource_type = if item.types.is_empty() {
                    ops::stake::ResourceType::BANDWIDTH
                } else {
                    ops::stake::ResourceType::ENERGY
                };
                resp::UnfreezeListResp::new(
                    item.unfreeze_amount,
                    resource_type,
                    item.unfreeze_expire_time,
                )
            })
            .collect::<Vec<resp::UnfreezeListResp>>();

        result.sort_by_key(|r| std::cmp::Reverse(r.available_at));

        Ok(result)
    }

    // 解除质押
    pub async fn un_freeze_balance(
        &self,
        req: stake::UnFreezeBalanceReq,
        password: &str,
    ) -> Result<resp::FreezeResp, crate::error::ServiceError> {
        let from = req.owner_address.clone();

        // 解质押的时候可能存在待提取的,提示给前端
        let can_widthdraw = self
            .chain
            .provider
            .can_withdraw_unfreeze_amount(&req.owner_address)
            .await?;

        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::UnFreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::UnFreezeEnergy,
        };

        let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, 0, 0.0, &req.signer, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.unfreeze_balance, resource_type)
            .await?;

        let date = wallet_utils::time::now_plus_days(14);
        let resource = resp::ResourceResp::new(req.unfreeze_balance, resource_type, resource_value);

        Ok(
            resp::FreezeResp::new(req.owner_address, resource, tx_hash, bill_kind)
                .expiration_at(date)
                .withdraw_amount(can_widthdraw.to_sun()),
        )
    }

    // 取消解锁
    pub async fn cancel_all_unfreeze(
        &self,
        req: stake::CancelAllUnFreezeReq,
        password: String,
    ) -> Result<resp::CancelAllUnFreezeResp, crate::ServiceError> {
        let account = self.chain.account_info(&req.owner_address).await?;
        let resource = ChainTransDomain::account_resource(&self.chain, &req.owner_address).await?;

        // 1可以解质押的带宽
        let bandwidth = account.un_freeze_amount("");

        // 2.可以解质押的能量
        let energy = account.un_freeze_amount("ENERGY");
        let args = ops::stake::CancelAllFreezeBalanceArgs::new(
            &req.owner_address,
            req.signer.clone().map(|s| s.permission_id),
        )?;

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::CancelAllUnFreeze,
                &req.owner_address,
                0,
                (bandwidth + energy) as f64,
                &req.signer,
                &password,
            )
            .await?;

        let res = resp::CancelAllUnFreezeResp {
            owner_address: req.owner_address.to_string(),
            votes: bandwidth + energy,
            energy: resource.resource_value(ops::stake::ResourceType::ENERGY, energy)?,
            bandwidth: resource.resource_value(ops::stake::ResourceType::BANDWIDTH, bandwidth)?,
            amount: account.can_withdraw_amount(),
            tx_hash,
        };
        Ok(res)
    }

    // 提取金额
    pub async fn withdraw_unfreeze(
        &self,
        req: stake::WithdrawBalanceReq,
        password: String,
    ) -> Result<resp::WithdrawUnfreezeResp, crate::error::ServiceError> {
        let can_widthdraw = self
            .chain
            .provider
            .can_withdraw_unfreeze_amount(&req.owner_address)
            .await?;

        if can_widthdraw.amount <= 0 {
            return Err(crate::BusinessError::Stake(
                crate::StakeError::NoWithdrawableAmount,
            ))?;
        }

        let args = ops::stake::WithdrawUnfreezeArgs {
            owner_address: req.owner_address.to_string(),
            permission_id: req.signer.clone().map(|s| s.permission_id),
        };

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::WithdrawUnFreeze,
                &req.owner_address,
                0,
                can_widthdraw.to_sun() as f64,
                &req.signer,
                &password,
            )
            .await?;

        Ok(resp::WithdrawUnfreezeResp {
            amount: can_widthdraw.to_sun(),
            owner_address: req.owner_address.to_string(),
            tx_hash,
        })
    }

    pub async fn account_resource(
        &self,
        owner: &str,
    ) -> Result<AccountResource, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let resource = chain.account_resource(owner).await?;
        let account = chain.account_info(owner).await?;

        let mut res: AccountResource = AccountResource {
            balance: account.balance_to_f64(),
            ..Default::default()
        };

        res.votes = resource.tron_power_limit;
        res.freeze_num = account.frozen_v2.len() as i64 - 1;
        res.pending_withdraw = account.can_withdraw_num();
        // 解锁中的不包括带提取的
        res.unfreeze_num = account.unfreeze_v2.len() as i64 - res.pending_withdraw;

        // bandwidth
        res.bandwidth.total_resource = resource.net_limit + resource.free_net_limit;
        res.bandwidth.limit_resource =
            (res.bandwidth.total_resource - resource.free_net_used - resource.net_used).max(0);

        // 一笔交易的资源消耗
        let consumer = EstimateTxConsumer::new(&chain).await?;
        res.bandwidth.consumer = consumer.bandwidth;
        res.energy.consumer = consumer.energy;

        self.fill_resource_info(
            &mut res.bandwidth,
            &account,
            ops::stake::ResourceType::BANDWIDTH,
            resource.net_price(),
        );

        // energy
        res.energy.total_resource = resource.energy_limit;
        res.energy.limit_resource = (resource.energy_limit - resource.energy_used).max(0);
        self.fill_resource_info(
            &mut res.energy,
            &account,
            ops::stake::ResourceType::ENERGY,
            resource.energy_price(),
        );

        // 当前用户的法币
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        // 当前的币价
        let token_price =
            TokenCurrencyGetter::get_currency(currency, chain_code::TRON, "TRX").await?;
        let unit_price = token_price.get_price(currency);

        // 总质押
        let amount = res.bandwidth.owner_freeze.amount + res.energy.owner_freeze.amount;
        let balance = BalanceInfo::new(amount as f64, unit_price, currency);
        res.total_freeze = balance;

        // 总的解冻中的
        let amount = res.bandwidth.un_freeze.amount + res.energy.un_freeze.amount;
        let balance = BalanceInfo::new(amount as f64, unit_price, currency);
        res.total_un_freeze = balance;

        // res.total_un_freeze = balance;
        // 总的待提取
        let amount = res.bandwidth.pending_withdraw.amount + res.energy.pending_withdraw.amount;
        res.total_pending_widthdraw = BalanceInfo::new(amount as f64, unit_price, currency);

        res.delegate_num = res.bandwidth.delegate_freeze.amount + res.energy.delegate_freeze.amount;

        // 总共质押了多少(包含解冻中的)
        res.freeze_amount = (res.total_freeze.amount + res.total_un_freeze.amount) as i64;

        Ok(res)
    }

    //******************************************************delegate **************************************************/
    // 验证地址是否在初始化过
    pub async fn account_exists(
        &self,
        accounts: Vec<String>,
    ) -> Result<Vec<resp::AddressExists>, crate::ServiceError> {
        let mut res = vec![];

        for account in accounts {
            let account_info = self.chain.account_info(&account).await?;
            let exists = !account_info.address.is_empty();

            res.push(AddressExists {
                address: account,
                exists,
            });
        }

        Ok(res)
    }

    pub async fn can_delegated_max(
        &self,
        account: String,
        resource_type: String,
        is_multisig: Option<bool>,
    ) -> Result<resp::ResourceResp, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;
        let res = chain
            .provider
            .can_delegate_resource(account.as_str(), resource_type)
            .await?;

        let mut value = res.to_sun();

        let resource = ChainTransDomain::account_resource(&self.chain, &account).await?;
        let price = match resource_type {
            ResourceType::BANDWIDTH => resource.net_price(),
            ResourceType::ENERGY => resource.energy_price(),
        };

        let mut resource_value = (price * value as f64 * 100.0).round() / 100.0;

        // 多签的代理带宽在进行扣减
        if matches!(is_multisig, Some(true)) && 0 == resource_type.to_i8() {
            // 默认扣减 3个签名的长度 在加 锁定期的长度
            let reduce = (3 * 67 + 5) as f64;
            value = (value - (reduce / price) as i64).max(0);
            resource_value = (resource_value - (3 * 67 + 5) as f64).max(0.0);
        }

        Ok(resp::ResourceResp::new(
            value,
            resource_type,
            resource_value,
        ))
    }

    // 委派资源给账号
    pub async fn delegate_resource(
        &self,
        req: stake::DelegateReq,
        password: &str,
    ) -> Result<resp::DelegateResp, crate::ServiceError> {
        let from = req.owner_address.clone();

        if req.balance <= 0 {
            return Err(crate::BusinessError::Stake(
                crate::StakeError::DelegateLessThanMin,
            ))?;
        }

        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::DelegateBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::DelegateEnergy,
        };
        let args = ops::stake::DelegateArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, 0, 0.0, &req.signer, &password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.balance, resource_type, resource_value);

        Ok(resp::DelegateResp::new_delegate(
            req, resource, bill_kind, tx_hash,
        ))
    }

    pub async fn min_remiaing_time(
        &self,
        from: String,
        to: Vec<String>,
        resource_type: String,
    ) -> Result<DelegateRemaingTime, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;
        let now = wallet_utils::time::now().timestamp_millis();

        let mut times = vec![];
        for address in to.iter() {
            let time = self
                .min_time_for_delegate(&from, address, resource_type, now)
                .await?;

            if let Some(time) = time {
                times.push(time);
            }
        }

        let time = *times.iter().min().unwrap_or(&0);

        let days = if time > 0 {
            let diff_days = (time - now) as f64 / 86400000.0;
            (diff_days * 100.0).round() / 100.0
        } else {
            0.0
        };

        Ok(DelegateRemaingTime { days })
    }

    async fn min_time_for_delegate(
        &self,
        from: &str,
        to: &str,
        resource_type: ops::stake::ResourceType,
        now: i64,
    ) -> Result<Option<i64>, crate::ServiceError> {
        let mut res = StakeDomain::get_delegate_info(from, to, &self.chain).await?;

        let time = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => {
                res.delegated_resource.sort_by(|a, b| {
                    a.expire_time_for_bandwidth
                        .cmp(&b.expire_time_for_bandwidth)
                });

                res.delegated_resource.iter().find_map(|d| {
                    (d.expire_time_for_bandwidth > now).then_some(d.expire_time_for_bandwidth)
                })
            }
            ops::stake::ResourceType::ENERGY => {
                res.delegated_resource
                    .sort_by(|a, b| a.expire_time_for_energy.cmp(&b.expire_time_for_energy));

                res.delegated_resource.iter().find_map(|d| {
                    (d.expire_time_for_energy > now).then_some(d.expire_time_for_energy)
                })
            }
        };

        Ok(time)
    }

    async fn process_batch<T>(
        &self,
        resource_type: ops::stake::ResourceType,
        owner_address: &str,
        bill_kind: BillKind,
        password: &str,
        args: Vec<impl ops::TronTxOperation<T>>,
        amount: i64,
        signer: &Option<Signer>,
    ) -> Result<BatchDelegateResp, crate::ServiceError> {
        let resource = ChainTransDomain::account_resource(&self.chain, owner_address).await?;
        let chain_param = self.chain.get_provider().chain_params().await?;

        // 批量构建交易
        let txs: Vec<TempBuildTransaction> = self
            .batch_build_transaction(args, bill_kind, &chain_param, &resource)
            .await?;

        // check balance
        let balance = self.chain.balance(owner_address, None).await?;
        let fee = txs
            .iter()
            .map(|item| item.conusmer.transaction_fee_i64())
            .sum::<i64>();

        if balance.to::<i64>() < fee {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientFeeBalance,
            ))?;
        }

        let key = self.get_key(owner_address, signer, password).await?;
        let res = self.batch_exec(owner_address, key, bill_kind, txs).await?;

        let resource_value = resource.resource_value(resource_type, amount)?;
        let resource = ResourceResp::new(amount, resource_type, resource_value);

        Ok(BatchDelegateResp::new(
            owner_address.to_string(),
            res,
            resource,
            bill_kind,
        ))
    }

    pub async fn batch_delegate(
        &self,
        req: stake::BatchDelegate,
        password: String,
    ) -> Result<BatchDelegateResp, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(req.resource_type.as_str())?;

        for list in req.list.iter() {
            if list.value <= 0 {
                return Err(crate::BusinessError::Stake(
                    crate::StakeError::DelegateLessThanMin,
                ))?;
            }
        }

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::BatchDelegateBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::BatchDelegateEnergy,
        };

        let args: Vec<DelegateArgs> = (&req).try_into()?;
        let amount = req.total();

        self.process_batch(
            resource_type,
            &req.owner_address,
            bill_kind,
            &password,
            args,
            amount,
            &req.signer,
        )
        .await
    }

    pub async fn batch_un_delegate(
        &self,
        req: stake::BatchUnDelegate,
        password: String,
    ) -> Result<BatchDelegateResp, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(req.resource_type.as_str())?;

        for list in req.list.iter() {
            if list.value <= 0 {
                return Err(crate::BusinessError::Stake(
                    crate::StakeError::UnDelegateLessThanMin,
                ))?;
            }
        }

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::BatchUnDelegateBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::BatchUnDelegateEnergy,
        };

        let args: Vec<UnDelegateArgs> = (&req).try_into()?;
        let amount = req.total();

        self.process_batch(
            resource_type,
            &req.owner_address,
            bill_kind,
            &password,
            args,
            amount,
            &req.signer,
        )
        .await
    }

    // Reclaim delegated energy
    pub async fn un_delegate_resource(
        &self,
        req: stake::UnDelegateReq,
        password: String,
    ) -> Result<resp::DelegateResp, crate::error::ServiceError> {
        let from = req.owner_address.clone();

        if req.balance <= 0 {
            return Err(crate::BusinessError::Stake(
                crate::StakeError::UnDelegateLessThanMin,
            ))?;
        }

        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;
        let args = ops::stake::UnDelegateArgs::try_from(&req)?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::UnDelegateBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::UnDelegateEnergy,
        };

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, 0, 0.0, &req.signer, &password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.balance, resource_type, resource_value);
        Ok(resp::DelegateResp::new_undelegate(
            req, resource, bill_kind, tx_hash,
        ))
    }

    pub async fn delegate_to_other(
        &self,
        owner_address: &str,
        resource_type: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateListResp>, crate::ServiceError> {
        // 查询所有的代理
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let delegate_other = chain.provider.delegate_others_list(owner_address).await?;

        let resource = ChainTransDomain::account_resource(&chain, owner_address).await?;

        let len = delegate_other.to_accounts.len();
        let total_page = len / page_size as usize;

        if page as usize > total_page {
            let res = Pagination {
                page,
                page_size,
                total_count: len as i64,
                data: vec![],
            };
            return Ok(res);
        }

        let mut data = vec![];
        let accounts = self.page_address(page, page_size, &delegate_other.to_accounts);
        for to in accounts {
            let res = self
                .process_delegate(&chain, &resource_type, owner_address, &to, &resource)
                .await?;
            data.extend(res);
        }

        let res = Pagination {
            page,
            page_size,
            total_count: len as i64,
            data,
        };

        Ok(res)
    }

    fn page_address(&self, page: i64, page_size: i64, accounts: &[String]) -> Vec<String> {
        let start = (page) * page_size;
        let end = ((page + 1) * page_size) as usize;

        let len = accounts.len();
        let end = if end > len { len } else { end };

        accounts[start as usize..end].to_vec()
    }

    pub async fn delegate_from_other(
        &self,
        to: &str,
        resource_type: Option<String>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateListResp>, crate::ServiceError> {
        // 查询所有的代理
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let delegate_other = chain.provider.delegate_others_list(to).await?;

        let len = delegate_other.from_accounts.len();
        let total_page = len / page_size as usize;
        if page as usize > total_page {
            let res = Pagination {
                page,
                page_size,
                total_count: len as i64,
                data: vec![],
            };
            return Ok(res);
        }

        let resource = ChainTransDomain::account_resource(&chain, to).await?;

        let mut data = vec![];
        for owner_address in self.page_address(page, page_size, &delegate_other.from_accounts) {
            let res = self
                .process_delegate(&chain, &resource_type, &owner_address, to, &resource)
                .await?;
            data.extend(res);
        }

        let res = Pagination {
            page,
            page_size,
            total_count: len as i64,
            data,
        };

        Ok(res)
    }

    pub async fn system_resource(
        &self,
        address: String,
    ) -> Result<SystemEnergyResp, crate::error::ServiceError> {
        let backhand = Context::get_global_backend_api()?;
        let aes_cbc_cryptor = crate::manager::Context::get_global_aes_cbc_cryptor()?;
        let req = serde_json::json!({
            "address": address
        });
        let res = backhand
            .post_request::<_, SystemEnergyResp>("delegate/resource/limit", req, aes_cbc_cryptor)
            .await?;

        Ok(res)
    }

    pub async fn request_energy(
        &self,
        account: String,
        energy: i64,
    ) -> Result<String, crate::error::ServiceError> {
        let backhand = Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;

        // 验证后端的配置(是否开启了能量的补偿)
        if !backhand.delegate_is_open(cryptor).await? {
            return Err(BusinessError::Stake(StakeError::SwitchClose))?;
        }

        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        // request resource
        let res = backhand.delegate_order(cryptor, &account, energy).await?;
        if !res.energy_status {
            return Err(BusinessError::Stake(StakeError::DelegateEnergyFailed))?;
        }

        let duration = tokio::time::Duration::from_millis(500);

        let mut query_time = 0;
        let mut energy_status = 0; // 0 initial status  1:failed 2:success
        loop {
            if energy_status < 2 {
                if let Some(hash) = &res.energy_hash {
                    energy_status = match chain.query_tx_res(hash).await {
                        Ok(Some(rs)) if rs.status == 2 => 2,
                        Ok(_) => 1,
                        Err(_) => 1,
                    };
                }
            }

            if query_time > 2 || (energy_status == 2) {
                break;
            };
            query_time += 1;

            tracing::warn!("sleep query result of delegate");
            tokio::time::sleep(duration).await;
        }

        Ok(res.order_id)
    }

    pub async fn vote_list(
        &self,
        owner_address: Option<&str>,
    ) -> Result<resp::VoteListResp, crate::error::ServiceError> {
        // 所有的超级节点列表
        // let chain = self.chain.get_provider();
        // let mut res = StakeDomain::vote_list(chain).await?;
        let mut res = StakeDomain::vote_list_from_backend().await?;
        if let Some(owner_address) = owner_address {
            let account_info = self.chain.account_info(owner_address).await?;
            // 投票人投票列表
            let votes = account_info.votes;
            // 遍历超级节点列表，判断投票人是否已经给该超级节点投票，然后设置Witness的vote_count字段
            res.data.iter_mut().for_each(|witness| {
                if let Some(vote) = votes.iter().find(|v| v.vote_address == witness.address) {
                    witness.vote_count_by_owner = Some(vote.vote_count);
                }
            });
            res.sort_data();
        }

        Ok(res)
    }

    pub async fn voter_info(
        &self,
        owner: &str,
    ) -> Result<resp::VoterInfoResp, crate::error::ServiceError> {
        // let chain = self.chain.get_provider();
        // let vote_list = StakeDomain::vote_list(chain).await?;
        let vote_list = StakeDomain::vote_list_from_backend().await?;
        let reward = self.chain.get_provider().get_reward(owner).await?;

        let account_info = self.chain.account_info(owner).await?;
        // tracing::info!("account_info: {:#?}", account_info);

        let balance = account_info.balance_to_f64();
        let votes = account_info.votes;

        let mut representatives = Vec::new();
        for vote in votes.iter() {
            let vote_info = vote_list
                .data
                .iter()
                .find(|x| x.address == *vote.vote_address);
            if let Some(vote_info) = vote_info {
                representatives.push(domain::stake::Representative::new(
                    vote.vote_count as f64,
                    vote_info.apr,
                ));
                // tracing::info!("vote_info: {:#?}", vote_info);
            }
        }
        // tracing::info!("representatives: {:#?}", representatives);
        let comprehensive_apr = StakeDomain::calculate_comprehensive_apr(representatives);

        let resource = self.chain.account_resource(owner).await?;
        let res = resp::VoterInfoResp::new(
            balance,
            reward.to_sun(),
            resource.tron_power_limit,
            resource.tron_power_used,
            // votes.into(),
            comprehensive_apr,
        );
        Ok(res)
    }

    pub async fn top_witness(&self) -> Result<Option<resp::Witness>, crate::error::ServiceError> {
        // let chain = self.chain.get_provider();
        // let list = StakeDomain::vote_list(chain).await?.data;
        let list = StakeDomain::vote_list_from_backend().await?.data;
        // 获取最大投票数的witness
        let mut max_apr = 0.0f64;
        let mut max_witness = None;
        for witness in list.iter() {
            if witness.apr > max_apr {
                max_apr = witness.apr;
                max_witness = Some(witness);
            }
        }

        Ok(max_witness.cloned())
    }

    pub async fn votes(
        &self,
        req: stake::VoteWitnessReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let args = ops::stake::VoteWitnessArgs::try_from(&req)?;

        let bill_value = req.get_votes() as f64;

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::Vote,
                &req.owner_address,
                0,
                bill_value,
                &req.signer,
                password,
            )
            .await?;

        Ok(tx_hash)
    }

    pub async fn votes_claim_rewards(
        &self,
        req: stake::WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let mut args = ops::stake::WithdrawBalanceArgs::try_from(&req)?;

        let value = self
            .chain
            .get_provider()
            .get_reward(&req.owner_address)
            .await?
            .to_sun();

        args.value = Some(value);
        if value < 0.0 {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::NoRewardClaim,
            ))?;
        }

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::WithdrawReward,
                &req.owner_address,
                0,
                0.0,
                &req.signer,
                password,
            )
            .await?;

        Ok(tx_hash)
    }

    // 质押相关构建多签交易
    pub async fn build_multisig_stake(
        &mut self,
        bill_kind: i64,
        content: String,
        expiration: i64,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let bill_kind = BillKind::try_from(bill_kind as i8)?;
        // 转换多签的参数
        let (args, address, amount, signer) = self.convert_stake_args(bill_kind, content).await?;

        // 走权限流程
        if let Some(signer) = signer {
            self.create_with_permission(
                address, args, bill_kind, amount, expiration, password, signer,
            )
            .await
        } else {
            self.create_with_account(address, args, bill_kind, amount, expiration, password)
                .await
        }
    }

    async fn create_with_permission(
        &self,
        grantor_addr: String,
        args: StakeArgs,
        bill_kind: BillKind,
        amount: f64,
        expiration: i64,
        password: String,
        signer: Signer,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let to = args.get_to();

        let permission =
            PermissionRepo::permission_with_user(&pool, &grantor_addr, signer.permission_id, false)
                .await?;

        let Some(p) = permission else {
            return Err(crate::BusinessError::Permission(
                crate::PermissionError::ActivesPermissionNotFound,
            ))?;
        };

        let expiration = MultisigQueueDomain::sub_expiration(expiration);
        let rs = args
            .build_multisig_tx(&self.chain, expiration as u64)
            .await?;

        let mut queue = NewMultisigQueueEntity::new(
            "".to_string(),
            grantor_addr.to_string(),
            to,
            expiration,
            &rs.tx_hash,
            &rs.raw_data,
            bill_kind,
            amount.to_string(),
        );
        queue.permission_id = p.permission.id.clone();

        // 对多签队列进行签名
        MultisigQueueDomain::batch_sign_with_permission(&mut queue, &password, &p, &pool).await?;

        queue.compute_status(p.permission.threshold as i32);

        let res = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut queue).await?;

        let opt = PermissionData {
            opt_address: signer.address.clone(),
            users: p.users(),
        };

        // 上报后端
        MultisigQueueDomain::upload_queue_backend(res.id, &pool, None, Some(opt)).await?;

        Ok(rs.tx_hash)
    }

    async fn create_with_account(
        &self,
        address: String,
        args: StakeArgs,
        bill_kind: BillKind,
        amount: f64,
        expiration: i64,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let to = args.get_to();

        let account = MultisigDomain::account_by_address(&address, true, &pool).await?;
        MultisigDomain::validate_queue(&account)?;

        // 构建多签交易
        let expiration = MultisigQueueDomain::sub_expiration(expiration);
        let resp = args
            .build_multisig_tx(&self.chain, expiration as u64)
            .await?;

        let mut queue = NewMultisigQueueEntity::new(
            account.id.to_string(),
            address.to_string(),
            to,
            expiration,
            &resp.tx_hash,
            &resp.raw_data,
            bill_kind,
            amount.to_string(),
        );

        let res = MultisigQueueDomain::tron_sign_and_create_queue(
            &mut queue,
            &account,
            password,
            pool.clone(),
        )
        .await?;

        MultisigQueueDomain::upload_queue_backend(res.id, &pool, None, None).await?;
        Ok(resp.tx_hash)
    }
}
