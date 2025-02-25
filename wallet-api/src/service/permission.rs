use crate::{
    domain::{self, account::open_account_pk_with_password, chain::adapter::ChainAdapterFactory},
    notify::{
        event::other::{Process, TransactionProcessFrontend},
        FrontendNotifyEvent, NotifyEvent,
    },
    request::permission::PermissionReq,
    response_vo::permssion::{AccountPermission, PermissionList, PermissionResp},
};
use wallet_chain_interact::{
    tron::{
        operations::{multisig::Permission, permisions::PermissionUpdateArgs, TronTxOperation},
        TronChain,
    },
    BillResourceConsume,
};
use wallet_database::entities::bill::{BillKind, NewBillEntity};
use wallet_types::constant::chain_code;

pub struct PermssionService {
    chain: TronChain,
}

impl PermssionService {
    pub async fn new() -> Result<Self, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        Ok(Self { chain })
    }

    async fn update_permision<T>(
        &self,
        bill_kind: BillKind,
        from: &str,
        args: impl TronTxOperation<T>,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            bill_kind,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        // 手续拦截
        let resp = args.build_raw_transaction(&self.chain.provider).await?;
        // 验证余额
        let balance = self.chain.balance(from, None).await?;
        let consumer = self
            .chain
            .get_provider()
            .transfer_fee(from, None, &resp.raw_data_hex, 1)
            .await?;

        if balance.to::<i64>() < consumer.transaction_fee_i64() {
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

        let key = open_account_pk_with_password(chain_code::TRON, from, password).await?;
        let hash = self.chain.exec_transaction_v1(resp, key).await?;

        let transaction_fee = consumer.transaction_fee();
        // 写入本地交易数据

        let bill_consumer = BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0);
        let entity = NewBillEntity::new_stake_bill(
            hash.clone(),
            from.to_string(),
            args.get_to(),
            args.get_value(),
            bill_kind,
            bill_consumer.to_json_str()?,
            transaction_fee,
        );
        domain::bill::BillDomain::create_bill(entity).await?;

        Ok(hash)
    }
}

impl PermssionService {
    // all permission category
    pub fn permission_list() -> Result<PermissionList, crate::ServiceError> {
        Ok(PermissionList::default())
    }
    // trans permssion
    pub fn permssion_trans() -> Result<Vec<i8>, crate::ServiceError> {
        Ok(PermissionList::trans_permission())
    }

    // account permisson
    pub async fn account_permssion(
        &self,
        address: String,
    ) -> Result<AccountPermission, crate::ServiceError> {
        let account = self.chain.account_info(&address).await?;

        let actives = account
            .active_permission
            .iter()
            .map(|p| PermissionResp::try_from(p).unwrap())
            .collect();

        Ok(AccountPermission {
            owner: PermissionResp::try_from(&account.owner_permission)?,
            actives,
        })
    }

    // add new permission
    pub async fn add_permission(
        &self,
        req: PermissionReq,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // check
        let account = self.chain.account_info(&req.address).await?;
        if account.active_permission.len() > 7 {
            return Err(crate::BusinessError::Permisison(
                crate::PermissionError::ActivesPermissionMore,
            ))?;
        };

        req.check_threshold()?;

        let mut args = PermissionUpdateArgs::try_from(&account)?;

        let permission = Permission::try_from(&req)?;

        // 新增权限
        if let Some(actives) = args.actives.as_mut() {
            actives.push(permission);
        } else {
            args.actives = Some(vec![permission])
        }

        let result = self
            .update_permision(BillKind::UpdatgePermission, &req.address, args, &password)
            .await?;

        Ok(result)
    }

    // update new permission
    pub async fn up_permission(
        &self,
        req: PermissionReq,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // check
        let account = self.chain.account_info(&req.address).await?;

        req.check_threshold()?;

        let id = req.permission_id.unwrap();

        let mut args = PermissionUpdateArgs::try_from(&account)?;
        let permission = Permission::try_from(&req)?;

        // 新增权限
        if let Some(actives) = args.actives.as_mut() {
            for item in actives.iter_mut() {
                if item.id.unwrap_or_default() == id {
                    *item = permission;
                    break;
                }
            }
        }

        // 验证是否还有active 是否为空
        // 验证是否修改

        let result = self
            .update_permision(BillKind::UpdatgePermission, &req.address, args, &password)
            .await?;

        Ok(result)
    }

    // update new permission
    pub async fn del_permission(
        &self,
        address: String,
        id: i8,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // check
        let account = self.chain.account_info(&address).await?;

        let mut args = PermissionUpdateArgs::try_from(&account)?;

        // 删除权限
        if let Some(actives) = args.actives.as_mut() {
            actives.retain(|item| item.id.unwrap_or_default() != id);
        }

        // 验证是否还有active 是否为空
        // 验证是否删除

        let result = self
            .update_permision(BillKind::UpdatgePermission, &address, args, &password)
            .await?;

        Ok(result)
    }
}
