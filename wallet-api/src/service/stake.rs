use crate::domain;
use crate::domain::chain::adapter::ChainAdapterFactory;
use crate::domain::chain::transaction::ChainTransaction;
use crate::domain::coin::TokenCurrencyGetter;
use crate::error::business::stake::StakeError;
use crate::manager::Context;
use crate::notify::event::other::Process;
use crate::notify::event::other::TransactionProcessFrontend;
use crate::notify::FrontendNotifyEvent;
use crate::notify::NotifyEvent;
use crate::request::stake;
use crate::request::stake::CancelAllUnFreezeReq;
use crate::request::stake::UnDelegateReq;
use crate::request::stake::VoteWitnessReq;
use crate::request::stake::WithdrawBalanceReq;
use crate::response_vo::account::AccountResource;
use crate::response_vo::account::Resource;
use crate::response_vo::account::TrxResource;
use crate::response_vo::stake as resp;
use crate::response_vo::stake::ResourceResp;
use crate::response_vo::EstimateFeeResp;
use crate::response_vo::TronFeeDetails;
use crate::BusinessError;
use wallet_chain_interact::tron::operations as ops;
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;
use wallet_chain_interact::tron::protocol::account::TronAccount;
use wallet_chain_interact::tron::TronChain;
use wallet_database::dao::bill::BillDao;
use wallet_database::entities::bill::BillKind;
use wallet_database::entities::bill::NewBillEntity;
use wallet_transport_backend::response_vo::stake::SystemEnergyResp;
use wallet_types::constant::chain_code;

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

                let resource = ResourceResp::new(amount, resource_type, resource_value);
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

                let resource = ResourceResp::new(amount, resource_type, resource_value);
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

        // 计算总的质押
        resource.calculate_total();
    }
}

// 单笔交易需要花费的能量
pub const NET_CONSUME: f64 = 270.0;
// 代币转账消耗的能量
pub const ENERGY_CONSUME: f64 = 70000.0;

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

        match bill_kind {
            BillKind::FreezeBandwidth | BillKind::FreezeEnergy => {
                let req =
                    wallet_utils::serde_func::serde_from_str::<stake::FreezeBalanceReq>(&content)?;
                let args = ops::stake::FreezeBalanceArgs::try_from(&req)?;

                let consumer = self.chain.simple_fee(&req.owner_address, 1, args).await?;
                let res = TronFeeDetails::new(consumer, token_currency, currency)?;
                let content = wallet_utils::serde_func::serde_to_string(&res)?;

                Ok(EstimateFeeResp::new(
                    "TRX".to_string(),
                    "tron".to_string(),
                    content,
                ))
            }
            BillKind::UnFreezeBandwidth | BillKind::UnFreezeEnergy => {
                let req = wallet_utils::serde_func::serde_from_str::<stake::UnFreezeBalanceReq>(
                    &content,
                )?;
                let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

                let consumer = self.chain.simple_fee(&req.owner_address, 1, args).await?;
                let res = TronFeeDetails::new(consumer, token_currency, currency)?;
                let content = wallet_utils::serde_func::serde_to_string(&res)?;

                Ok(EstimateFeeResp::new(
                    "TRX".to_string(),
                    "tron".to_string(),
                    content,
                ))
            }
            BillKind::CancelAllUnFreeze => {
                let req = wallet_utils::serde_func::serde_from_str::<stake::CancelAllUnFreezeReq>(
                    &content,
                )?;
                let args = ops::stake::CancelAllFreezeBalanceArgs::new(&req.owner_address)?;

                let consumer = self.chain.simple_fee(&req.owner_address, 1, args).await?;
                let res = TronFeeDetails::new(consumer, token_currency, currency)?;
                let content = wallet_utils::serde_func::serde_to_string(&res)?;

                Ok(EstimateFeeResp::new(
                    "TRX".to_string(),
                    "tron".to_string(),
                    content,
                ))
            }

            _ => {
                panic!("xx");
            }
        }
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

            let resource_resp = ResourceResp::new(bandwidth, resource_type, resource_value);
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

            let resource_resp = ResourceResp::new(energy, resource_type, resource_value);
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
        req: CancelAllUnFreezeReq,
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
        req: WithdrawBalanceReq,
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

        Ok(ResourceResp::new(value, resource_type, resource_value))
    }

    // 委派资源给账号
    pub async fn delegate_resource(
        &self,
        req: stake::DelegateReq,
        password: &str,
    ) -> Result<resp::DelegateResp, crate::error::ServiceError> {
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

        let resource = ResourceResp::new(req.balance, resource_type, resource_value);

        Ok(resp::DelegateResp::new_delegate(
            req, resource, bill_kind, tx_hash,
        ))
    }

    // Reclaim delegated energy
    pub async fn un_delegate_resource(
        &self,
        req: UnDelegateReq,
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

        let resource = ResourceResp::new(req.balance, resource_type, resource_value);
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

    pub async fn votes_fee_estimation(
        &self,
        req: VoteWitnessReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        // let args = ops::stake::VoteWitnessArgs::try_from(&req)?;

        // let tx_hash = self
        //     .process_transaction(args, BillKind::Vote, &req.owner_address, password)
        //     .await?;
        todo!();

        // Ok(tx_hash)
    }

    // #[derive(serde::Deserialize, serde::Serialize, Debug)]
    // pub struct ListWitnessResp {
    //     pub witnesses: Vec<Witness>,
    // }

    // #[derive(serde::Deserialize, serde::Serialize, Debug)]
    // #[serde(rename_all = "camelCase")]
    // pub struct Witness {
    //     pub address: String,
    //     pub vote_count: Option<i64>,
    //     pub url: String,
    //     total_produced: Option<i64>,
    //     total_missed: Option<i64>,
    //     latest_block_num: Option<i64>,
    //     latest_slot_num: Option<i64>,
    //     is_jobs: Option<bool>,
    // }
    pub async fn vote_list(
        &self,
        req: VoteWitnessReq,
    ) -> Result<
        wallet_transport_backend::response_vo::stake::ListWitnessResp,
        crate::error::ServiceError,
    > {
        let list = self.chain.get_provider().list_witnesses().await?;

        //总票数
        let total_votes = list
            .witnesses
            .iter()
            .map(|w| w.vote_count.unwrap_or(0))
            .sum::<i64>();

        // let mut result = vec![];
        // for w in list.witnesses {
        //     let brokerage = self.chain.get_provider().get_brokerage(&w.address).await?;
        //     let witness = wallet_transport_backend::response_vo::stake::Witness::new(
        //         &w.address,
        //         w.vote_count,
        //         &w.url,
        //         brokerage.brokerages,
        //     );
        //     result.push(resp::VoteWitnessResp::from_witness(&witness, brokerage));
        // }

        todo!()
        // Ok(list)
    }

    pub async fn votes(
        &self,
        req: VoteWitnessReq,
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
        req: WithdrawBalanceReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let args = ops::stake::WithdrawBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::Vote, &req.owner_address, password)
            .await?;

        Ok(tx_hash)
    }
}
