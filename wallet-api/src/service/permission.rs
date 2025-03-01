use crate::{
    domain::{
        self, account::open_account_pk_with_password, chain::adapter::ChainAdapterFactory,
        coin::TokenCurrencyGetter,
    },
    mqtt::payload::incoming::permission::PermissionAccept,
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

    async fn upload_backend(
        &self,
        permission: PermissionAccept,
        tx_hash: String,
    ) -> Result<(), crate::ServiceError> {
        let req = PermissionAcceptReq {
            hash: tx_hash,
            tx_str: permission.to_json_val()?,
        };

        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let backend = crate::Context::get_global_backend_api()?;

        backend.permission_accept(req, &cryptor).await?;
        Ok(())
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
    ) -> Result<Permission, crate::ServiceError> {
        req.check_threshold()?;
        match types {
            PermissionReq::NEW => {
                if args.actives.len() > 7 {
                    return Err(crate::BusinessError::Permisison(
                        crate::PermissionError::ActivesPermissionMore,
                    ))?;
                };

                let permission = Permission::try_from(req)?;
                args.actives.push(permission.clone());
                Ok(permission)
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
                    *permission = new_permission.clone();
                } else {
                    return Err(crate::BusinessError::Permisison(
                        crate::PermissionError::ActviesPermissionNotFound,
                    ))?;
                }

                Ok(new_permission)
            }
            PermissionReq::DELETE => {
                let active_id = req.active_id.unwrap_or_default();
                // check exists
                let permission = args
                    .actives
                    .iter()
                    .find(|a| a.id.unwrap_or_default() == active_id)
                    .cloned();

                tracing::warn!("p{:#?}", permission);

                if let Some(permission) = permission {
                    // // 删除权限
                    // args.actives
                    //     .retain(|item| item.id.unwrap_or_default() != active_id);

                    if args.actives.len() == 0 {
                        return Err(crate::BusinessError::Permisison(
                            crate::PermissionError::MissActivesPermission,
                        ))?;
                    }

                    Ok(permission)
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

        let _ = self.build_args(&mut args, &types, &req)?;

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

        // let modified_permission = self.build_args(&mut args, &types, &req)?;

        tracing::warn!("{:#?}", args);
        assert!(false);
        let tx_hash = self
            .update_permision(&req.grantor_addr, args, &password)
            .await?;

        // let tx_hash =
        //     "5cae1129bc8cda4e1c70c298be1bf4a97444bd27c72044b09da9442c57d13e55".to_string();

        // let permission =
        //     PermissionAccept::try_from((&modified_permission, req.grantor_addr.as_str()))?;

        // 上报后端
        // self.upload_backend(permission, tx_hash.clone()).await?;

        Ok(tx_hash)
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
