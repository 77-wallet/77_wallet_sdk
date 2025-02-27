use crate::{
    domain::{self, account::open_account_pk_with_password, chain::adapter::ChainAdapterFactory},
    notify::{
        event::other::{Process, TransactionProcessFrontend},
        FrontendNotifyEvent, NotifyEvent,
    },
    request::permission::PermissionReq,
    response_vo::permssion::{AccountPermission, Keys, PermissionList, PermissionResp},
};
use wallet_chain_interact::{
    tron::{
        consts,
        operations::{multisig::Permission, permisions::PermissionUpdateArgs, TronTxOperation},
        TronChain,
    },
    BillResourceConsume,
};
use wallet_database::{
    entities::bill::{BillKind, NewBillEntity},
    repositories::{address_book::AddressBookRepo, permission::PermissionRepo},
    DbPool,
};
use wallet_types::constant::chain_code;

pub struct PermssionService {
    chain: TronChain,
}

impl PermssionService {
    pub async fn new() -> Result<Self, crate::ServiceError> {
        let chain = ChainAdapterFactory::get_tron_adapter().await?;
        Ok(Self { chain })
    }

    // 标记使用地址簿里面的名字
    pub async fn mark_address_book_name(
        &self,
        pool: &DbPool,
        keys: &mut [Keys],
    ) -> Result<(), crate::ServiceError> {
        for key in keys.iter_mut() {
            let book = AddressBookRepo::find_by_address_chain(pool, &key.address, chain_code::TRON)
                .await?;
            if let Some(book) = book {
                key.name = book.name;
            }
        }

        Ok(())
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
        let mut consumer = self
            .chain
            .get_provider()
            .transfer_fee(from, None, &resp.raw_data_hex, 1)
            .await?;

        // upgrade fee
        consumer.set_extra_fee(100 * consts::TRX_VALUE);

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
    pub async fn account_permission(
        &self,
        address: String,
    ) -> Result<AccountPermission, crate::ServiceError> {
        let account = self.chain.account_info(&address).await?;

        let actives = account
            .active_permission
            .iter()
            .map(|p| PermissionResp::try_from(p))
            .collect::<Result<Vec<PermissionResp>, _>>()?;

        let mut result = AccountPermission {
            owner: PermissionResp::try_from(&account.owner_permission)?,
            actives,
        };

        let pool = crate::Context::get_global_sqlite_pool()?;

        self.mark_address_book_name(&pool, &mut result.owner.keys)
            .await?;

        for item in result.actives.iter_mut() {
            self.mark_address_book_name(&pool, &mut item.keys).await?;
        }

        Ok(result)
    }

    // 我管理的权限
    pub async fn manager_permision(&self) -> Result<Vec<PermissionResp>, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        let permissions = PermissionRepo::all_permission_with_user(&pool).await?;

        let mut result = permissions
            .iter()
            .map(|p| PermissionResp::try_from(p))
            .collect::<Result<Vec<PermissionResp>, crate::ServiceError>>()?;

        for item in result.iter_mut() {
            self.mark_address_book_name(&pool, &mut item.keys).await?;
        }

        Ok(result)
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

        // 新增权限
        let permission = Permission::try_from(&req)?;
        args.actives.push(permission);

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

        let mut args = PermissionUpdateArgs::try_from(&account)?;

        // update permission
        let id = req.permission_id.unwrap();
        let new_permission = Permission::try_from(&req)?;
        if let Some(permission) = args
            .actives
            .iter_mut()
            .find(|p| p.id.unwrap_or_default() == id)
        {
            *permission = new_permission;
        } else {
            return Err(crate::BusinessError::Permisison(
                crate::PermissionError::ActviesPermissionNotFound,
            ))?;
        }

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
        let account = self.chain.account_info(&address).await?;

        let mut args = PermissionUpdateArgs::try_from(&account)?;

        // check exists
        let exists = args.actives.iter().any(|a| a.id.unwrap_or_default() == id);
        if !exists {
            return Err(crate::BusinessError::Permisison(
                crate::PermissionError::ActviesPermissionNotFound,
            ))?;
        };

        // 删除权限
        args.actives
            .retain(|item| item.id.unwrap_or_default() != id);

        if args.actives.len() == 0 {
            return Err(crate::BusinessError::Permisison(
                crate::PermissionError::MissActivesPermission,
            ))?;
        }

        let result = self
            .update_permision(BillKind::UpdatgePermission, &address, args, &password)
            .await?;

        Ok(result)
    }
}
