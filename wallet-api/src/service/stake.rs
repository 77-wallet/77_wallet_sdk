use crate::domain;
use crate::domain::chain::adapter::ChainAdapterFactory;
use crate::domain::chain::transaction::ChainTransaction;
use crate::domain::coin::TokenCurrencyGetter;
use crate::domain::multisig::MultisigDomain;
use crate::domain::stake::StakeArgs;
use crate::domain::task_queue::Tasks;
use crate::error::business::stake::StakeError;
use crate::manager::Context;
use crate::mqtt::payload::incoming::transaction::MultiSignTransAccept;
use crate::notify::event::other::Process;
use crate::notify::event::other::TransactionProcessFrontend;
use crate::notify::FrontendNotifyEvent;
use crate::notify::NotifyEvent;
use crate::request::stake;
use crate::response_vo::account::AccountResource;
use crate::response_vo::account::Resource;
use crate::response_vo::account::TrxResource;
use crate::response_vo::account::ENERGY_CONSUME;
use crate::response_vo::account::NET_CONSUME;
use crate::response_vo::stake as resp;
use crate::response_vo::EstimateFeeResp;
use crate::response_vo::TronFeeDetails;
use crate::BusinessError;
use wallet_chain_interact::tron::operations as ops;
use wallet_chain_interact::tron::operations::multisig::TransactionOpt;
use wallet_chain_interact::tron::operations::stake::DelegateArgs;
use wallet_chain_interact::tron::operations::stake::UnDelegateArgs;
use wallet_chain_interact::tron::operations::RawTransactionParams;
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;
use wallet_chain_interact::tron::protocol::account::TronAccount;
use wallet_chain_interact::tron::TronChain;
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::dao::bill::BillDao;
use wallet_database::entities::bill::BillKind;
use wallet_database::entities::bill::NewBillEntity;
use wallet_database::entities::multisig_queue::MultisigQueueEntity;
use wallet_database::entities::multisig_queue::NewMultisigQueueEntity;
use wallet_database::entities::multisig_signatures::MultisigSignatureStatus;
use wallet_database::entities::multisig_signatures::NewSignatureEntity;
use wallet_database::repositories::multisig_queue::MultisigQueueRepo;
use wallet_transport_backend::consts::endpoint;
use wallet_transport_backend::request::SignedTranCreateReq;
use wallet_transport_backend::response_vo::stake::SystemEnergyResp;
use wallet_types::constant::chain_code;
use wallet_utils::serde_func;

pub struct StackService {
    chain: TronChain,
}

impl StackService {
    pub async fn new() -> Result<Self, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        Ok(Self { chain })
    }

    async fn process_transaction<T>(
        &self,
        args: impl ops::TronTxOperation<T>,
        bill_kind: BillKind,
        from: &str,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        // 构建交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let resp = args.build_raw_transaction(&self.chain.provider).await?;

        // 广播交易交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let key = domain::account::open_account_pk_with_password(chain_code::TRON, from, password)
            .await?;
        let hash = self.chain.exec_transaction_v1(resp, key).await?;

        // 写入本地交易数据
        let entity = NewBillEntity::new_stake_bill(hash.clone(), from.to_string(), bill_kind);
        domain::bill::BillDomain::create_bill(entity).await?;

        Ok(hash)
    }

    // 批量构建交易并发送通知到前端
    async fn batch_build_transaction<T>(
        &self,
        args: Vec<impl ops::TronTxOperation<T>>,
        bill_kind: BillKind,
    ) -> Result<Vec<RawTransactionParams>, crate::ServiceError> {
        let mut result = vec![];

        for (i, item) in args.into_iter().enumerate() {
            let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new_with_num(
                bill_kind,
                (i + 1) as i64,
                Process::Building,
            ));
            FrontendNotifyEvent::new(data).send().await?;

            let res = item.build_raw_transaction(&self.chain.provider).await?;
            result.push(res);
        }

        Ok(result)
    }

    async fn batch_exec<T>(
        &self,
        from: String,
        key: ChainPrivateKey,
        bill_kind: BillKind,
        txs: Vec<RawTransactionParams>,
    ) -> Result<Vec<String>, crate::ServiceError> {
        let mut exce_res = vec![];

        for (i, item) in txs.into_iter().enumerate() {
            let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new_with_num(
                bill_kind,
                (i + 1) as i64,
                Process::Building,
            ));
            FrontendNotifyEvent::new(data).send().await?;

            exce_res.push("".to_string());
            // let result = self.chain.exec_transaction_v1(item, key.clone()).await;
            // match result {
            //     Ok(hash) => {
            //         let entity =
            //             NewBillEntity::new_stake_bill(hash.clone(), from.clone(), bill_kind);
            //         domain::bill::BillDomain::create_bill(entity).await?;
            //     }
            //     Err(e) => {}
            // }
        }
        Ok(exce_res)
    }

    async fn resource_value(
        &self,
        owner_address: &str,
        value: i64,
        resource_type: ops::stake::ResourceType,
    ) -> Result<f64, crate::ServiceError> {
        let resource = ChainTransaction::account_resorce(&self.chain, owner_address).await?;
        Ok(resource.resource_value(resource_type, value)?)
    }

    async fn process_delegate(
        &self,
        chain: &TronChain,
        owner_address: &str,
        to: &str,
        resource: &AccountResourceDetail,
    ) -> Result<Vec<resp::DelegateListResp>, crate::ServiceError> {
        let mut result = Vec::new();

        let res = chain
            .provider
            .delegated_resource(owner_address, &to)
            .await?;

        for delegate in res.delegated_resource {
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
        let owner = account.frozen_v2_owner(&type_str);
        resource.owner_freeze = TrxResource::new(owner, price);

        resource.can_unfreeze = owner;

        // 可提取的
        let can_withdraw = account.can_withdraw_unfreeze_amount(&type_str);
        resource.pending_withdraw = TrxResource::new(can_withdraw, price);

        // 计算可转账次数
        resource.calculate_transfer_times(resource_type);

        // 计算总的质押
        resource.calculate_total();
    }

    fn convert_stake_args(
        &self,
        bill_kind: BillKind,
        content: String,
    ) -> Result<(StakeArgs, String), crate::ServiceError> {
        match bill_kind {
            BillKind::FreezeBandwidth | BillKind::FreezeEnergy => {
                let req = serde_func::serde_from_str::<stake::FreezeBalanceReq>(&content)?;
                let args = ops::stake::FreezeBalanceArgs::try_from(&req)?;

                Ok((StakeArgs::Freeze(args), req.owner_address.clone()))
            }
            BillKind::UnFreezeBandwidth | BillKind::UnFreezeEnergy => {
                let req = serde_func::serde_from_str::<stake::UnFreezeBalanceReq>(&content)?;
                let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

                Ok((StakeArgs::UnFreeze(args), req.owner_address.clone()))
            }
            BillKind::CancelAllUnFreeze => {
                let req = serde_func::serde_from_str::<stake::CancelAllUnFreezeReq>(&content)?;
                let args = ops::stake::CancelAllFreezeBalanceArgs::new(&req.owner_address)?;

                Ok((
                    StakeArgs::CancelAllUnFreeze(args),
                    req.owner_address.clone(),
                ))
            }
            BillKind::WithdrawUnFreeze => {
                let req = serde_func::serde_from_str::<stake::WithdrawBalanceReq>(&content)?;
                let args = ops::stake::WithdrawBalanceArgs {
                    owner_address: req.owner_address.clone(),
                };

                Ok((StakeArgs::Withdraw(args), req.owner_address.clone()))
            }
            BillKind::DelegateBandwidth | BillKind::DelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::DelegateReq>(&content)?;
                let args = ops::stake::DelegateArgs::try_from(&req)?;

                Ok((StakeArgs::Delegate(args), req.owner_address.clone()))
            }
            BillKind::UnDelegateBandwidth | BillKind::UnDelegateEnergy => {
                let req = serde_func::serde_from_str::<stake::UnDelegateReq>(&content)?;
                let args = ops::stake::UnDelegateArgs::try_from(&req)?;

                Ok((StakeArgs::UnDelegate(args), req.owner_address.clone()))
            }
            BillKind::Vote => {
                let req = serde_func::serde_from_str::<stake::VoteWitnessReq>(&content)?;
                let args = ops::stake::VoteWitnessArgs::try_from(&req)?;

                Ok((StakeArgs::Votes(args), req.owner_address.clone()))
            }
            BillKind::WithdrawReward => {
                let req = serde_func::serde_from_str::<stake::WithdrawBalanceReq>(&content)?;
                let args = ops::stake::WithdrawBalanceArgs::try_from(&req)?;

                Ok((StakeArgs::Withdraw(args), req.owner_address.clone()))
            }
            _ => {
                return Err(crate::BusinessError::Stake(
                    crate::StakeError::UnSupportBillKind,
                ))?
            }
        }
    }
}

impl StackService {
    // 输入trx 得到对应的资源
    pub async fn trx_to_resource(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> Result<resp::TrxToResourceResp, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;

        let resource_value = self.resource_value(&account, value, resource_type).await?;
        let resource = resp::ResourceResp::new(value, resource_type, resource_value);

        let consumer = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => NET_CONSUME,
            ops::stake::ResourceType::ENERGY => ENERGY_CONSUME,
        };

        Ok(resp::TrxToResourceResp::new(resource, consumer))
    }

    // 输入 resoruce 换算trx
    pub async fn resource_to_trx(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> Result<resp::ResourceToTrxResp, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;
        let resource = ChainTransaction::account_resorce(&self.chain, &account).await?;

        let (price, consumer) = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => (resource.net_price(), NET_CONSUME),
            ops::stake::ResourceType::ENERGY => (resource.energy_price(), ENERGY_CONSUME),
        };

        let amount = (value as f64 / price) as i64;
        let transfer_times = value as f64 / consumer;

        Ok(resp::ResourceToTrxResp {
            amount,
            votes: amount,
            transfer_times,
        })
    }

    pub async fn estimate_stake_fee(
        &self,
        bill_kind: i64,
        content: String,
    ) -> Result<EstimateFeeResp, crate::ServiceError> {
        let bill_kind = BillKind::try_from(bill_kind as i8)?;

        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();

        let token_currency =
            domain::coin::token_price::TokenCurrencyGetter::get_currency(currency, "tron", "TRX")
                .await?;

        let (args, account) = self.convert_stake_args(bill_kind, content)?;

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
            .process_transaction(args, bill_kind, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.frozen_balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.frozen_balance, resource_type, resource_value);
        Ok(resp::FreezeResp::new(resource, tx_hash, bill_kind))
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
        let bandwidth = account.frozen_v2_owner("");
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

        let energy = account.frozen_v2_owner("ENERGY");
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

        let mut result = account
            .unfreeze_v2
            .iter()
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
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::UnFreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::UnFreezeEnergy,
        };

        let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.unfreeze_balance, resource_type)
            .await?;

        let date = wallet_utils::time::now_plus_days(14);
        let resource = resp::ResourceResp::new(req.unfreeze_balance, resource_type, resource_value);

        Ok(resp::FreezeResp::new(resource, tx_hash, bill_kind).expiration_at(date))
    }

    // 取消解锁
    pub async fn cancel_all_unfreeze(
        &self,
        req: stake::CancelAllUnFreezeReq,
        password: String,
    ) -> Result<resp::CancelAllUnFreezeResp, crate::ServiceError> {
        let account = self.chain.account_info(&req.owner_address).await?;
        let resource = ChainTransaction::account_resorce(&self.chain, &req.owner_address).await?;

        // 1可以解质押的带宽
        let bandwidth = account.can_withdraw_unfreeze_amount("");

        // 2.可以解质押的能量
        let energy = account.can_withdraw_unfreeze_amount("ENERGY");
        let args = ops::stake::CancelAllFreezeBalanceArgs::new(&req.owner_address)?;

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::CancelAllUnFreeze,
                &req.owner_address,
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
        };

        let tx_hash = self
            .process_transaction(
                args,
                BillKind::WithdrawUnFreeze,
                &req.owner_address,
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
        account: &str,
    ) -> Result<AccountResource, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let resource = chain.account_resource(account).await?;
        let account = chain.account_info(account).await?;

        let mut res: AccountResource = AccountResource {
            balance: account.balance_to_f64(),
            ..Default::default()
        };

        res.votes = resource.tron_power_limit;
        res.freeze_num = account.frozen_v2.len() as i64 - 1;
        res.unfreeze_num = account.unfreeze_v2.len() as i64;
        res.pending_withdraw = account.can_withdraw_num();

        // bandwitdh
        res.bandwidth.total_resource = resource.net_limit + resource.free_net_limit;
        res.bandwidth.limit_resource = res.bandwidth.total_resource - resource.free_net_used;

        self.fill_resource_info(
            &mut res.bandwidth,
            &account,
            ops::stake::ResourceType::BANDWIDTH,
            resource.net_price(),
        );

        // energy
        res.energy.total_resource = resource.energy_limit;
        res.energy.limit_resource = resource.energy_limit - resource.energy_used;
        self.fill_resource_info(
            &mut res.energy,
            &account,
            ops::stake::ResourceType::ENERGY,
            resource.energy_price(),
        );

        let amount = res.bandwidth.total_freeze.amount + res.energy.total_freeze.amount;

        let balance =
            TokenCurrencyGetter::get_balance_info(chain_code::TRON, "TRX", amount as f64).await?;
        res.total_freeze = balance;

        Ok(res)
    }

    //******************************************************delegate **************************************************/
    pub async fn can_delegated_max(
        &self,
        account: String,
        resource_type: String,
    ) -> Result<resp::ResourceResp, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;
        let res = chain
            .provider
            .can_delegate_resource(account.as_str(), resource_type)
            .await?;

        let value = res.to_sun();
        let resource_value = self.resource_value(&account, value, resource_type).await?;

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
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::FreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::FreezeEnergy,
        };
        let args = ops::stake::DelegateArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.balance, resource_type, resource_value);

        Ok(resp::DelegateResp::new_delegate(
            req, resource, bill_kind, tx_hash,
        ))
    }

    pub async fn batch_delegate(
        &self,
        req: stake::BulkDelegate,
        password: String,
    ) -> Result<(), crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(req.resource_type.as_str())?;
        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::FreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::FreezeEnergy,
        };
        let bill_kind = BillKind::BatchDelegateBandwidth;

        // 批量构建交易
        let args: Vec<DelegateArgs> = (&req).try_into()?;
        let txs = self.batch_build_transaction(args, bill_kind).await?;

        // let res = self
        //     .batch_exec(req.owner_address, key, bill_kind, txs)
        //     .await;

        Ok(())
    }

    pub async fn batch_un_delegate(
        &self,
        req: stake::BulkUnDelegate,
        password: String,
    ) -> Result<(), crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(req.resource_type.as_str())?;
        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::FreezeBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::FreezeEnergy,
        };
        let bill_kind = BillKind::BatchDelegateBandwidth;

        // 批量构建交易
        let args: Vec<UnDelegateArgs> = (&req).try_into()?;
        let txs = self.batch_build_transaction(args, bill_kind).await?;

        Ok(())
    }

    // Reclaim delegated energy
    pub async fn un_delegate_resource(
        &self,
        req: stake::UnDelegateReq,
        password: String,
    ) -> Result<resp::DelegateResp, crate::error::ServiceError> {
        let from = req.owner_address.clone();
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;
        let args = ops::stake::UnDelegateArgs::try_from(&req)?;

        let bill_kind = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => BillKind::UnDelegateBandwidth,
            ops::stake::ResourceType::ENERGY => BillKind::UnDelegateEnergy,
        };

        let tx_hash = self
            .process_transaction(args, bill_kind, &from, &password)
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
    ) -> Result<Vec<resp::DelegateListResp>, crate::ServiceError> {
        // 查询所有的代理
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let delegate_other = chain.provider.delegate_others_list(owner_address).await?;

        let resource = ChainTransaction::account_resorce(&chain, owner_address).await?;

        let mut result = vec![];
        for to in delegate_other.to_accounts {
            let res = self
                .process_delegate(&chain, &owner_address, &to, &resource)
                .await?;
            result.extend(res);
        }

        Ok(result)
    }

    pub async fn delegate_from_other(
        &self,
        to: &str,
    ) -> Result<Vec<resp::DelegateListResp>, crate::ServiceError> {
        // 查询所有的代理
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let delegate_other = chain.provider.delegate_others_list(to).await?;

        let resource = ChainTransaction::account_resorce(&chain, to).await?;

        let mut result = vec![];
        for owner_address in delegate_other.from_accounts {
            let res = self
                .process_delegate(&chain, &owner_address, to, &resource)
                .await?;
            result.extend(res);
        }

        Ok(result)
    }

    pub async fn system_resource(
        &self,
        address: String,
    ) -> Result<SystemEnergyResp, crate::error::ServiceError> {
        let backhand = Context::get_global_backend_api()?;

        let req = serde_json::json!({
            "address": address
        });
        let res = backhand
            .post_request::<_, SystemEnergyResp>("delegate/resource/limit", req)
            .await?;

        Ok(res)
    }

    pub async fn request_energy(
        &self,
        account: String,
        energy: i64,
    ) -> Result<String, crate::error::ServiceError> {
        let backhand = Context::get_global_backend_api()?;

        // 验证后端的配置(是否开启了能量的补偿)
        if !backhand.delegate_is_open().await? {
            return Err(BusinessError::Stake(StakeError::SwitchClose))?;
        }

        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        // request resource
        let res = backhand.delegate_order(&account, energy).await?;

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
    ) -> Result<ops::stake::vote_list::VoteWitnessResp, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_scan_adapter().await?;
        let list = chain.get_provider().vote_witnesses().await?;
        tracing::info!("list witness: {:?}", list);

        Ok(list)
    }

    pub async fn voter_info(
        &self,
        owner: &str,
    ) -> Result<ops::stake::VoteRewardResp, crate::error::ServiceError> {
        // let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let reward = self.chain.get_provider().get_reward(owner).await?;

        let account_info = self.chain.account_info(owner).await?;
        tracing::info!("account_info: {:#?}", account_info);

        let resource = self.chain.account_resource(owner).await?;
        let res = ops::stake::VoteRewardResp::new(
            account_info.balance_to_f64(),
            reward.to_sun(),
            resource.tron_power_limit,
            resource.tron_power_used,
        );
        Ok(res)
    }

    pub async fn vote_top_rewards(
        &self,
    ) -> Result<Option<ops::stake::vote_list::Witness>, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_scan_adapter().await?;
        let witness = chain
            .get_provider()
            .vote_witnesses()
            .await?
            .data
            .first()
            .cloned();
        tracing::info!("witness: {:?}", witness);

        Ok(witness)
    }

    pub async fn votes(
        &self,
        req: stake::VoteWitnessReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let args = ops::stake::VoteWitnessArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::Vote, &req.owner_address, password)
            .await?;

        Ok(tx_hash)
    }

    pub async fn votes_claim_rewards(
        &self,
        req: stake::WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let args = ops::stake::WithdrawBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::Vote, &req.owner_address, password)
            .await?;

        Ok(tx_hash)
    }

    // 质押相关构建多签交易
    pub async fn build_multisig_stake(
        &self,
        bill_kind: i64,
        content: String,
        expiration: i64,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let queue_repo = MultisigQueueRepo::new(pool.clone());

        let bill_kind = BillKind::try_from(bill_kind as i8)?;
        // 转换多签的参数
        let (args, address) = self.convert_stake_args(bill_kind, content)?;

        let account = MultisigDomain::account_by_address(&address, true, &pool).await?;
        MultisigDomain::validate_queue(&account)?;

        let expiration = expiration * 3600;

        // 构建多签交易
        let resp = args
            .build_multisig_tx(&self.chain, expiration as u64)
            .await?;

        let expiration = (wallet_utils::time::now().timestamp() + expiration) as u64;
        let mut queue = NewMultisigQueueEntity::new(
            account.id.to_string(),
            address.to_string(),
            expiration as i64,
            &resp.tx_hash,
            &resp.raw_data,
            bill_kind,
        );

        let mut members = queue_repo.self_member_account_id(&account.id).await?;
        members.prioritize_by_address(&account.initiator_addr);

        // sign num
        let sign_num = members.0.len().min(account.threshold as usize);
        for i in 0..sign_num {
            let member = members.0.get(i).unwrap();
            let key = crate::domain::account::open_account_pk_with_password(
                chain_code::TRON,
                &member.address,
                &password,
            )
            .await?;

            let sign_result = TransactionOpt::sign_transaction(&resp.raw_data, key)?;
            let sign = NewSignatureEntity::new(
                &queue.id,
                &member.address,
                &sign_result.signature,
                MultisigSignatureStatus::Approved,
            );
            queue.signatures.push(sign);
        }

        queue.status =
            MultisigQueueEntity::compute_status(queue.signatures.len(), account.threshold as usize);
        let res = MultisigQueueRepo::create_queue_with_sign(pool.clone(), &mut queue).await?;

        // 上报后端
        let withdraw_id = res.id.clone();
        let sync_params = MultiSignTransAccept::from(res).with_signature(queue.signatures.clone());

        let raw_data = MultisigQueueRepo::multisig_queue_data(&withdraw_id, pool)
            .await?
            .to_string()?;
        let req = SignedTranCreateReq {
            withdraw_id,
            address,
            chain_code: chain_code::TRON.to_string(),
            tx_str: wallet_utils::serde_func::serde_to_string(&sync_params)?,
            raw_data,
        };

        let task =
            domain::task_queue::Task::BackendApi(domain::task_queue::BackendApiTask::BackendApi(
                domain::task_queue::BackendApiTaskData {
                    endpoint: endpoint::multisig::SIGNED_TRAN_CREATE.to_string(),
                    body: serde_func::serde_to_value(&req)?,
                },
            ));
        Tasks::new().push(task).send().await?;

        Ok(resp.tx_hash)
    }
}
