use crate::domain;
use crate::domain::chain::adapter::ChainAdapterFactory;
use crate::error::business::stake::StakeError;
use crate::manager::Context;
use crate::request::stake;
use crate::response_vo::account::AccountResource;
use crate::response_vo::stake::EstimatedResourcesResp;
use crate::response_vo::stake::FreezeListResp;
use crate::response_vo::stake::UnfreezeListResp;
use crate::BusinessError;
use wallet_chain_interact::tron::operations::stake::CancelAllFreezeBalanceArgs;
use wallet_chain_interact::tron::operations::stake::DelegateArgs;
use wallet_chain_interact::tron::operations::stake::FreezeBalanceArgs;
use wallet_chain_interact::tron::operations::stake::ResourceType;
use wallet_chain_interact::tron::operations::stake::UnFreezeBalanceArgs;
use wallet_chain_interact::tron::operations::TronTxOperation;
use wallet_database::entities::stake::DelegateEntity;
use wallet_database::pagination::Pagination;
use wallet_database::repositories::stake as db_stake;
use wallet_transport_backend::response_vo::stake::SystemEnergyResp;
use wallet_types::constant::chain_code;

pub struct StackService {
    repo: db_stake::StakeRepo,
}

impl StackService {
    pub fn new(repo: db_stake::StakeRepo) -> Self {
        Self { repo }
    }
}

impl StackService {
    pub async fn get_estimated_resources(
        &self,
        account: String,
        value: i64,
        resource_type: String,
    ) -> Result<EstimatedResourcesResp, crate::ServiceError> {
        let resource_type = ResourceType::try_from(resource_type.as_str())?;

        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let resource = chain.account_resource(&account).await?;

        let (price, consumer) = match resource_type {
            ResourceType::BANDWIDTH => (resource.net_price(), 268.0),
            ResourceType::ENERGY => (resource.energy_price(), 70000.0),
        };

        Ok(EstimatedResourcesResp::new(
            value,
            price,
            resource_type,
            consumer,
        ))
    }

    pub async fn freeze_balance(
        &self,
        req: stake::FreezeBalanceReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let key = domain::account::open_account_pk_with_password(
            chain_code::TRON,
            &req.owner_address,
            password,
        )
        .await?;

        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let args = FreezeBalanceArgs::try_from(req)?;
        let resp = args.build_raw_transaction(chain.get_provider()).await?;

        Ok(chain.exec_transaction_v1(resp, key).await?)
    }

    pub async fn freeze_list(
        &self,
        owner: &str,
    ) -> Result<Vec<FreezeListResp>, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let account = chain.account_info(owner).await?;
        let resource = chain.account_resource(owner).await?;

        let mut res = vec![];

        let bandwidth = account.frozen_v2_owner("");
        if bandwidth > 0 {
            let price = resource.net_price();
            let freeze = FreezeListResp::new(bandwidth, price, ResourceType::BANDWIDTH);
            res.push(freeze);
        }

        let energy = account.frozen_v2_owner("ENERGY");
        if energy > 0 {
            let energy_price = resource.energy_price();
            let freeze = FreezeListResp::new(energy, energy_price, ResourceType::ENERGY);
            res.push(freeze);
        }

        // TODO 匹配地址最新的交易记录里面的时间,需要匹配解质押

        Ok(res)
    }

    pub async fn un_freeze_list(
        &self,
        owner: &str,
    ) -> Result<Vec<UnfreezeListResp>, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        let account = chain.account_info(owner).await?;

        let mut result = account
            .unfreeze_v2
            .iter()
            .map(|item| {
                let resource = if item.types.is_empty() {
                    ResourceType::BANDWIDTH
                } else {
                    ResourceType::ENERGY
                };
                UnfreezeListResp::new(item.unfreeze_amount, resource, item.unfreeze_expire_time)
            })
            .collect::<Vec<UnfreezeListResp>>();

        result.sort_by_key(|r| std::cmp::Reverse(r.available_at));

        Ok(result)
    }

    pub async fn un_freeze_balance(
        &self,
        req: stake::UnFreezeBalanceReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let key = domain::account::open_account_pk_with_password(
            chain_code::TRON,
            &req.owner_address,
            password,
        )
        .await?;
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let args = UnFreezeBalanceArgs::try_from(req)?;
        let resp = args.build_raw_transaction(&chain.provider).await?;

        Ok(chain.exec_transaction_v1(resp, key).await?)
    }

    pub async fn cancel_all_unfreeze(
        &self,
        owner: &str,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let key =
            domain::account::open_account_pk_with_password(chain_code::TRON, &owner, password)
                .await?;
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let args = CancelAllFreezeBalanceArgs::new(owner)?;
        let resp = args.build_raw_transaction(&chain.provider).await?;

        Ok(chain.exec_transaction_v1(resp, key).await?)
    }

    pub async fn withdraw_unfreeze(
        &self,
        owner_address: &str,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let key = domain::account::open_account_pk_with_password(
            chain_code::TRON,
            &owner_address,
            password,
        )
        .await?;

        let res = chain.withdraw_unfreeze_amount(owner_address, key).await?;
        Ok(res)
    }

    pub async fn delegate_resource(
        &self,
        req: stake::DelegateReq,
        password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;

        let key = domain::account::open_account_pk_with_password(
            chain_code::TRON,
            &req.owner_address,
            password,
        )
        .await?;

        let args = DelegateArgs::try_from(req)?;

        let resp = args.build_raw_transaction(&chain.provider).await?;
        Ok(chain.exec_transaction_v1(resp, key).await?)
    }

    // Reclaim delegated energy
    pub async fn un_delegate_resource(
        &self,
        _id: String,
        _password: &str,
    ) -> Result<String, crate::error::ServiceError> {
        // let chain = ChainAdapterFactory::get_tron_adapter().await?;

        // let key = domain::account::open_account_pk_with_password(
        //     chain_code::TRON,
        //     &req.owner_address,
        //     password,
        // )
        // .await?;

        // let args = UnDelegateArgs::new(
        //     &delegate.owner_address,
        //     &delegate.receiver_address,
        //     &delegate.amount.to_string(),
        //     &delegate.resource_type,
        // )?;

        // let res = tron_chain.un_delegate_resource(args, key).await?;

        Ok("".to_string())
    }

    pub async fn delegate_list(
        &self,
        owner_address: &str,
        resource_type: &str,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<DelegateEntity>, crate::error::ServiceError> {
        Ok(self
            .repo
            .delegate_list(owner_address, resource_type, page, page_size)
            .await?)
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
}
