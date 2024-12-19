use crate::domain;
use crate::domain::chain::adapter::ChainAdapterFactory;
use crate::domain::chain::transaction::ChainTransaction;
use crate::error::business::stake::StakeError;
use crate::manager::Context;
use crate::notify::event::other::Process;
use crate::notify::event::other::TransactionProcessFrontend;
use crate::notify::FrontendNotifyEvent;
use crate::notify::NotifyEvent;
use crate::request::stake;
use crate::request::stake::UnDelegateReq;
use crate::response_vo::account::AccountResource;
use crate::response_vo::stake as resp;
use crate::BusinessError;
use wallet_chain_interact::tron::operations as ops;
use wallet_chain_interact::tron::protocol::account::AccountResourceDetail;
use wallet_chain_interact::tron::TronChain;
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

                let r = resp::DelegateListResp::new(
                    &delegate,
                    resource_value,
                    resource_type,
                    amount,
                    delegate.expire_time_for_energy,
                )?;
                result.push(r);
            }

            // 处理带宽类型
            if delegate.frozen_balance_for_bandwidth > 0 {
                let resource_type = ops::stake::ResourceType::BANDWIDTH;
                let amount = delegate.value_trx(resource_type);
                let resource_value = resource.resource_value(resource_type, amount)?;

                let r = resp::DelegateListResp::new(
                    &delegate,
                    resource_value,
                    resource_type,
                    amount,
                    delegate.expire_time_for_bandwidth,
                )?;
                result.push(r);
            }
        }

        Ok(result)
    }
}

impl StackService {
    // 预估可以获得的资源
    pub async fn get_estimated_resources(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> Result<resp::EstimatedResourcesResp, crate::ServiceError> {
        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;

        let resource = self.chain.account_resource(&account).await?;

        let (price, consumer) = match resource_type {
            ops::stake::ResourceType::BANDWIDTH => (resource.net_price(), 268.0),
            ops::stake::ResourceType::ENERGY => (resource.energy_price(), 70000.0),
        };

        Ok(resp::EstimatedResourcesResp::new(
            value,
            price,
            resource_type,
            consumer,
        ))
    }

    // 质押trx
    pub async fn freeze_balance(
        &self,
        req: stake::FreezeBalanceReq,
        password: &str,
    ) -> Result<resp::FreezeResp, crate::error::ServiceError> {
        let from = req.owner_address.clone();
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let args = ops::stake::FreezeBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::Freeze, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.frozen_balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.frozen_balance, resource_type, resource_value);
        Ok(resp::FreezeResp::new(resource, tx_hash, BillKind::Freeze))
    }

    pub async fn freeze_list(
        &self,
        owner: &str,
    ) -> Result<Vec<resp::FreezeListResp>, crate::error::ServiceError> {
        let account = self.chain.account_info(owner).await?;
        let resource = self.chain.account_resource(owner).await?;

        let mut res = vec![];
        let bandwidth = account.frozen_v2_owner("");
        if bandwidth > 0 {
            let price = resource.net_price();
            let freeze =
                resp::FreezeListResp::new(bandwidth, price, ops::stake::ResourceType::BANDWIDTH);
            res.push(freeze);
        }

        let energy = account.frozen_v2_owner("ENERGY");
        if energy > 0 {
            let energy_price = resource.energy_price();
            let freeze =
                resp::FreezeListResp::new(energy, energy_price, ops::stake::ResourceType::ENERGY);
            res.push(freeze);
        }

        // TODO 匹配地址最新的交易记录里面的时间,需要匹配解质押

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
                let resource = if item.types.is_empty() {
                    ops::stake::ResourceType::BANDWIDTH
                } else {
                    ops::stake::ResourceType::ENERGY
                };
                resp::UnfreezeListResp::new(
                    item.unfreeze_amount,
                    resource,
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

        let args = ops::stake::UnFreezeBalanceArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::UnFeeze, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.unfreeze_balance, resource_type)
            .await?;

        let resource = resp::ResourceResp::new(req.unfreeze_balance, resource_type, resource_value);
        Ok(resp::FreezeResp::new(resource, tx_hash, BillKind::UnFeeze))
    }

    // 取消解锁
    pub async fn cancel_all_unfreeze(
        &self,
        owner: &str,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let args = ops::stake::CancelAllFreezeBalanceArgs::new(owner)?;
        let tx_hash = self
            .process_transaction(args, BillKind::CancelAllUnFeeze, owner, password)
            .await?;

        // TODO

        Ok(tx_hash)
    }

    pub async fn withdraw_unfreeze(
        &self,
        owner_address: &str,
        password: &str,
    ) -> Result<resp::WithdrawUnfreezeResp, crate::error::ServiceError> {
        let can_widthdraw = self
            .chain
            .provider
            .can_withdraw_unfreeze_amount(owner_address)
            .await?;

        // TODO check 是否有足够的金额提现

        let args = ops::stake::WithdrawUnfreezeArgs {
            owner_address: owner_address.to_string(),
        };

        let tx_hash = self
            .process_transaction(args, BillKind::WithdrawUnFeeze, &owner_address, password)
            .await?;

        Ok(resp::WithdrawUnfreezeResp {
            amount: can_widthdraw.to_sun(),
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
            balance: (account.balance / 1_000_000).to_string(),
            ..Default::default()
        };

        // net
        res.bandwidth.total_resource = resource.net_limit + resource.free_net_limit;
        res.bandwidth.limit_resource = res.bandwidth.total_resource - resource.free_net_used;

        // 给别人质押的
        res.bandwidth
            .set_delegate_freeze_amount(account.delegated_bandwidth);
        // 别人给我的
        res.bandwidth
            .set_acquire_freeze_amount(account.acquired_bandwidth);
        // 自己质押的
        res.bandwidth
            .set_owner_freeze_amount(account.frozen_v2_owner(""));
        // 可解冻
        res.bandwidth
            .set_can_unfreeze_amount(account.frozen_v2_owner(""));
        // 可提现
        res.bandwidth
            .set_can_withdraw_unfreeze_amount(account.can_withdraw_unfreeze_amount(""));
        // 总共
        res.bandwidth.total_freeze_amount = res.bandwidth.delegate_freeze_amount
            + res.bandwidth.owner_freeze_amount
            + res.bandwidth.acquire_freeze_amount;
        res.bandwidth.price = resource.net_price();

        // energy
        res.energy.total_resource = resource.energy_limit;
        res.energy.limit_resource = resource.energy_limit - resource.energy_used;

        // 给别人质押的
        res.energy
            .set_delegate_freeze_amount(account.account_resource.delegated_energy);
        // 别人给我的
        res.energy
            .set_acquire_freeze_amount(account.account_resource.acquired_energy);
        // 自己质押的
        res.energy
            .set_owner_freeze_amount(account.frozen_v2_owner("ENERGY"));
        // 可解冻
        res.energy
            .set_can_unfreeze_amount(account.frozen_v2_owner("ENERGY"));
        // 可提现
        res.energy
            .set_can_withdraw_unfreeze_amount(account.can_withdraw_unfreeze_amount("ENERGY"));
        // 总共
        res.energy.total_freeze_amount = res.energy.delegate_freeze_amount
            + res.energy.owner_freeze_amount
            + res.energy.acquire_freeze_amount;
        res.energy.price = resource.energy_price();

        Ok(res)
    }

    //******************************************************delegate **************************************************/
    pub async fn can_delegated_max(
        &self,
        account: String,
        resource_type: String,
    ) -> Result<resp::CanDelegatedResp, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let resource_type = ops::stake::ResourceType::try_from(resource_type.as_str())?;
        let res = chain
            .provider
            .can_delegate_resource(account.as_str(), resource_type)
            .await?;

        Ok(resp::CanDelegatedResp {
            amount: res.to_sun(),
        })
    }

    // 委派资源给账号
    pub async fn delegate_resource(
        &self,
        req: stake::DelegateReq,
        password: &str,
    ) -> Result<resp::DelegateResp, crate::error::ServiceError> {
        let from = req.owner_address.clone();
        let resource_type = ops::stake::ResourceType::try_from(req.resource.as_str())?;

        let args = ops::stake::DelegateArgs::try_from(&req)?;

        let tx_hash = self
            .process_transaction(args, BillKind::Delegate, &from, password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.balance, resource_type)
            .await?;

        Ok(resp::DelegateResp::new_with_delegate(
            req,
            resource_value,
            resource_type,
            tx_hash,
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

        let tx_hash = self
            .process_transaction(args, BillKind::UnDelegate, &from, &password)
            .await?;

        let resource_value = self
            .resource_value(&req.owner_address, req.balance, resource_type)
            .await?;

        Ok(resp::DelegateResp::new_with_undegate(
            req,
            resource_value,
            resource_type,
            tx_hash,
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
}
