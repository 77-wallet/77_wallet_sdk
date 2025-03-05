use crate::{
    domain::{
        self,
        account::open_account_pk_with_password,
        chain::adapter::ChainAdapterFactory,
        coin::TokenCurrencyGetter,
        multisig::{MultisigDomain, MultisigQueueDomain},
    },
    notify::{
        event::other::{Process, TransactionProcessFrontend},
        FrontendNotifyEvent, NotifyEvent,
    },
    request::permission::PermissionReq,
    response_vo::{
        permssion::{AccountPermission, Keys, PermissionList, PermissionResp},
        EstimateFeeResp, TronFeeDetails,
    },
};
use alloy::primitives::map::HashSet;
use wallet_chain_interact::{
    tron::{
        consts,
        operations::{multisig::Permission, permisions::PermissionUpdateArgs, TronTxOperation},
        TronChain,
    },
    BillResourceConsume,
};
use wallet_database::{
    entities::{
        bill::{BillKind, NewBillEntity},
        multisig_queue::NewMultisigQueueEntity,
    },
    repositories::{address_book::AddressBookRepo, permission::PermissionRepo},
    DbPool,
};
use wallet_transport_backend::api::permission::PermissionAcceptReq;
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
        from: &str,
        args: impl TronTxOperation<T>,
        password: &str,
    ) -> Result<String, crate::ServiceError> {
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            BillKind::UpdatgePermission,
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
            BillKind::UpdatgePermission,
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
            BillKind::UpdatgePermission,
            bill_consumer.to_json_str()?,
            transaction_fee,
        );
        domain::bill::BillDomain::create_bill(entity).await?;

        Ok(hash)
    }

    async fn update_permission_fee<T: std::fmt::Debug + serde::Serialize>(
        &self,
        from: &str,
        args: impl TronTxOperation<T>,
    ) -> Result<EstimateFeeResp, crate::ServiceError> {
        let currency = crate::app_state::APP_STATE.read().await;
        let currency = currency.currency();
        let token_currency = TokenCurrencyGetter::get_currency(currency, "tron", "TRX").await?;

        // 预估手续费
        let mut consumer = self.chain.simple_fee(from, 1, args).await?;
        let chain_parameter = self.chain.provider.chain_params().await?;
        consumer.set_extra_fee(chain_parameter.update_account_fee());

        let res = TronFeeDetails::new(consumer, token_currency, currency)?;
        let content = wallet_utils::serde_func::serde_to_string(&res)?;

        Ok(EstimateFeeResp::new(
            "TRX".to_string(),
            chain_code::TRON.to_string(),
            content,
        ))
    }

    // 上报后端
    async fn upload_backend(&self, params: PermissionAcceptReq) -> Result<(), crate::ServiceError> {
        let aes_cbc_cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let backend = crate::Context::get_global_backend_api()?;

        Ok(backend.permission_accept(params, &aes_cbc_cryptor).await?)
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

    // 构建参数
    fn build_args(
        &self,
        args: &mut PermissionUpdateArgs,
        types: &str,
        req: &PermissionReq,
        backup_params: Option<&mut PermissionAcceptReq>,
    ) -> Result<(), crate::ServiceError> {
        req.check_threshold()?;
        match types {
            PermissionReq::NEW => {
                if args.actives.len() > 7 {
                    return Err(crate::BusinessError::Permisison(
                        crate::PermissionError::ActivesPermissionMore,
                    ))?;
                };

                let permission = Permission::try_from(req)?;

                // 拼接上报后端的参数
                if let Some(params) = backup_params {
                    params.current.types = PermissionReq::NEW.to_string();
                    params.current.name = req.name.to_string();
                    params.current.active_id = args.actives.len() as i64 + 2;
                    params.current.new_user = req.users();
                    params.current.operations = permission.operations.clone().unwrap_or_default();

                    params.sender_user = req.users();
                }

                args.actives.push(permission);
                Ok(())
            }
            PermissionReq::UPDATE => {
                // 对原来的进行调整
                let id = req.active_id.unwrap_or_default();
                let new_permission = Permission::try_from(req)?;

                if let Some(permission) = args
                    .actives
                    .iter_mut()
                    .find(|p| p.id.unwrap_or_default() == id)
                {
                    // 拼接上报后端的参数
                    if let Some(params) = backup_params {
                        let mut users = permission.users_from_hex()?;

                        params.current.original_user = users.clone();
                        params.current.types = PermissionReq::UPDATE.to_string();
                        params.current.name = req.name.to_string();
                        params.current.active_id = id as i64;
                        params.current.new_user = req.users();
                        params.current.operations =
                            permission.operations.clone().unwrap_or_default();

                        users.extend(req.users());

                        params.sender_user = users
                            .into_iter()
                            .collect::<HashSet<String>>()
                            .into_iter()
                            .collect();
                    }

                    *permission = new_permission;
                } else {
                    return Err(crate::BusinessError::Permisison(
                        crate::PermissionError::ActviesPermissionNotFound,
                    ))?;
                }

                Ok(())
            }
            PermissionReq::DELETE => {
                let active_id = req.active_id.unwrap_or_default();
                // check exists
                let permission = args
                    .actives
                    .iter()
                    .find(|a| a.id.unwrap_or_default() == active_id)
                    .cloned();

                if let Some(permission) = permission {
                    // 拼接上报后端的参数
                    if let Some(params) = backup_params {
                        let users = permission.users_from_hex()?;

                        params.current.original_user = users.clone();
                        params.current.types = PermissionReq::DELETE.to_string();
                        params.current.name = req.name.to_string();
                    }

                    // 删除权限
                    args.actives
                        .retain(|item| item.id.unwrap_or_default() != active_id);

                    if args.actives.len() == 0 {
                        return Err(crate::BusinessError::Permisison(
                            crate::PermissionError::MissActivesPermission,
                        ))?;
                    }

                    Ok(())
                } else {
                    return Err(crate::BusinessError::Permisison(
                        crate::PermissionError::ActviesPermissionNotFound,
                    ))?;
                }
            }
            _ => Err(crate::BusinessError::Permisison(
                crate::PermissionError::UnSupportOpType,
            ))?,
        }
    }

    pub async fn modify_permission_fee(
        &self,
        req: PermissionReq,
        types: String,
    ) -> Result<EstimateFeeResp, crate::ServiceError> {
        // 构建公用的参数
        let account = self.chain.account_info(&req.grantor_addr).await?;
        let mut args = PermissionUpdateArgs::try_from(&account)?;

        let _ = self.build_args(&mut args, &types, &req, None)?;

        self.update_permission_fee(&req.grantor_addr, args).await
    }

    pub async fn modify_permission(
        &self,
        req: PermissionReq,
        types: String,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // 构建公用的参数
        let account = self.chain.account_info(&req.grantor_addr).await?;
        let mut args = PermissionUpdateArgs::try_from(&account)?;

        // 上报后端的参数
        let mut backend_params = PermissionAcceptReq::default();
        if types == PermissionReq::DELETE {
            backend_params.sender_user = account.all_actives_user();
        }

        self.build_args(&mut args, &types, &req, Some(&mut backend_params))?;

        tracing::warn!("{:#?}", args);
        // 这个地址所有权限的用户集合
        let mut new_users = HashSet::new();
        for item in args.actives.iter() {
            for key in item.keys.iter() {
                new_users.insert(wallet_utils::address::hex_to_bs58_addr(&key.address)?);
            }
        }

        let tx_hash = self
            .update_permision(&req.grantor_addr, args, &password)
            .await?;

        backend_params.hash = tx_hash.clone();
        backend_params.grantor_addr = req.grantor_addr.clone();
        backend_params.back_user = new_users.into_iter().collect();

        tracing::warn!("{:#?}", backend_params);

        // 上报后端
        self.upload_backend(backend_params).await?;

        Ok(tx_hash)
    }

    pub async fn build_multisig_permission(
        &self,
        req: PermissionReq,
        types: String,
        expiration: i64,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let bill_kind = BillKind::UpdatgePermission;

        let account = MultisigDomain::account_by_address(&req.grantor_addr, true, &pool).await?;
        MultisigDomain::validate_queue(&account)?;

        let tron_account = self.chain.account_info(&req.grantor_addr).await?;
        let mut backend_params = PermissionAcceptReq::default();
        if types == PermissionReq::DELETE {
            backend_params.sender_user = tron_account.all_actives_user();
        }

        let mut args = PermissionUpdateArgs::try_from(&tron_account)?;
        self.build_args(&mut args, &types, &req, Some(&mut backend_params))?;

        // 这个地址所有权限的用户集合
        let mut new_users = HashSet::new();
        for item in args.actives.iter() {
            for key in item.keys.iter() {
                new_users.insert(wallet_utils::address::hex_to_bs58_addr(&key.address)?);
            }
        }

        // 构建多签交易
        let expiration = MultisigQueueDomain::sub_expiration(expiration);
        let resp = self
            .chain
            .build_multisig_transaction(args, expiration as u64)
            .await?;

        let mut queue = NewMultisigQueueEntity::new(
            account.id.to_string(),
            req.grantor_addr.to_string(),
            String::new(),
            expiration,
            &resp.tx_hash,
            &resp.raw_data,
            bill_kind,
            "0".to_string(),
        );

        let res = MultisigQueueDomain::tron_sign_and_create_queue(
            &mut queue,
            &account,
            password,
            pool.clone(),
        )
        .await?;

        backend_params.grantor_addr = req.grantor_addr.clone();
        backend_params.back_user = new_users.into_iter().collect();
        backend_params.multi_sign_id = res.id.clone();

        tracing::warn!("{:#?}", backend_params);

        MultisigQueueDomain::upload_queue_backend(res.id, &pool, Some(backend_params)).await?;

        Ok(resp.tx_hash)
    }
}

// pub async fn add_permission_fee(
//     &self,
//     req: PermissionReq,
// ) -> Result<EstimateFeeResp, crate::ServiceError> {
//     let account = self.chain.account_info(&req.grantor_addr).await?;
//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     let permission = Permission::try_from(&req)?;
//     args.actives.push(permission);

//     let result = self.update_permission_fee(&req.grantor_addr, args).await?;

//     Ok(result)
// }

// add new permission
// pub async fn add_permission(
//     &self,
//     req: PermissionReq,
//     password: String,
// ) -> Result<String, crate::ServiceError> {
//     // check
//     let account = self.chain.account_info(&req.grantor_addr).await?;
//     if account.active_permission.len() > 7 {
//         return Err(crate::BusinessError::Permisison(
//             crate::PermissionError::ActivesPermissionMore,
//         ))?;
//     };

//     req.check_threshold()?;

//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     // 新增权限
//     let permission = Permission::try_from(&req)?;
//     let permission_accept =
//         PermissionAccept::try_from((&permission, req.grantor_addr.as_str()))?;
//     args.actives.push(permission);

//     let tx_hash = self
//         .update_permision(&req.grantor_addr, args, &password)
//         .await?;

//     // 上报后端
//     self.upload_backend(permission_accept, tx_hash.clone(), "upsert".to_string())
//         .await?;

//     Ok(tx_hash)
// }

// pub async fn up_permission_fee(
//     &self,
//     req: PermissionReq,
// ) -> Result<EstimateFeeResp, crate::ServiceError> {
//     let account = self.chain.account_info(&req.grantor_addr).await?;

//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     let id = req.active_id.unwrap_or_default();
//     let new_permission = Permission::try_from(&req)?;

//     if let Some(permission) = args
//         .actives
//         .iter_mut()
//         .find(|p| p.id.unwrap_or_default() == id)
//     {
//         *permission = new_permission;
//     } else {
//         return Err(crate::BusinessError::Permisison(
//             crate::PermissionError::ActviesPermissionNotFound,
//         ))?;
//     }

//     let result = self.update_permission_fee(&req.grantor_addr, args).await?;

//     Ok(result)
// }

// update new permission
// pub async fn up_permission(
//     &self,
//     req: PermissionReq,
//     password: String,
// ) -> Result<String, crate::ServiceError> {
//     let account = self.chain.account_info(&req.grantor_addr).await?;
//     req.check_threshold()?;

//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     // update permission
//     let id = req.active_id.unwrap();
//     let new_permission = Permission::try_from(&req)?;
//     let permission = PermissionAccept::try_from((&new_permission, req.grantor_addr.as_str()))?;

//     if let Some(permission) = args
//         .actives
//         .iter_mut()
//         .find(|p| p.id.unwrap_or_default() == id)
//     {
//         *permission = new_permission;
//     } else {
//         return Err(crate::BusinessError::Permisison(
//             crate::PermissionError::ActviesPermissionNotFound,
//         ))?;
//     }

//     let tx_hash = self
//         .update_permision(&req.grantor_addr, args, &password)
//         .await?;

//     // 上报后端
//     self.upload_backend(permission, tx_hash.clone(), "upsert".to_string())
//         .await?;

//     Ok(tx_hash)
// }

// pub async fn del_permission_fee(
//     &self,
//     grantor_addr: String,
//     active_id: i8,
// ) -> Result<EstimateFeeResp, crate::ServiceError> {
//     let account = self.chain.account_info(&grantor_addr).await?;

//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     // check exists
//     let permission = args
//         .actives
//         .iter()
//         .find(|a| a.id.unwrap_or_default() == active_id)
//         .cloned();

//     if let Some(_permission) = permission {
//         // 删除权限
//         args.actives
//             .retain(|item| item.id.unwrap_or_default() != active_id);

//         if args.actives.len() == 0 {
//             return Err(crate::BusinessError::Permisison(
//                 crate::PermissionError::MissActivesPermission,
//             ))?;
//         }

//         let result = self.update_permission_fee(&grantor_addr, args).await?;
//         Ok(result)
//     } else {
//         return Err(crate::BusinessError::Permisison(
//             crate::PermissionError::ActviesPermissionNotFound,
//         ))?;
//     }
// }

// // update new permission
// pub async fn del_permission(
//     &self,
//     grantor_addr: String,
//     active_id: i8,
//     password: String,
// ) -> Result<String, crate::ServiceError> {
//     let account = self.chain.account_info(&grantor_addr).await?;

//     let mut args = PermissionUpdateArgs::try_from(&account)?;

//     // check exists
//     let permission = args
//         .actives
//         .iter()
//         .find(|a| a.id.unwrap_or_default() == active_id)
//         .cloned();

//     if let Some(permission) = permission {
//         // 删除权限
//         args.actives
//             .retain(|item| item.id.unwrap_or_default() != active_id);

//         if args.actives.len() == 0 {
//             return Err(crate::BusinessError::Permisison(
//                 crate::PermissionError::MissActivesPermission,
//             ))?;
//         }

//         let tx_hash = self
//             .update_permision(&grantor_addr, args, &password)
//             .await?;

//         let mut permission = PermissionAccept::try_from((&permission, grantor_addr.as_str()))?;
//         permission.permission.is_del = 1;
//         self.upload_backend(permission, tx_hash.clone(), "delete".to_string())
//             .await?;

//         Ok(tx_hash)
//     } else {
//         return Err(crate::BusinessError::Permisison(
//             crate::PermissionError::ActviesPermissionNotFound,
//         ))?;
//     }
// }
