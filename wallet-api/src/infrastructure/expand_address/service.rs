// service.rs
use wallet_database::repositories::api_wallet::{
    account::ApiAccountRepo, expand_batch::ExpandBatchRepo, wallet::ApiWalletRepo,
};
use wallet_transport_backend::request::{
    AddressInitReq,
    api_wallet::address::{ApiAddressInitReq, ExpandAddressCompleteReq},
};

use crate::{
    domain::api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    error::service::ServiceError,
    infrastructure::{
        expand_address::bootstrap::ExpandBootstrap,
        task_queue::{
            backend::{BackendApiTask, BackendApiTaskData},
            task::Tasks,
        },
    },
};

pub struct ExpandService;

impl ExpandService {
    pub(crate) async fn create_account(
        uid: &str,
        chain: &str,
        to_create: &[i32],
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let password = ApiWalletDomain::get_passwd().await?;
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?.ok_or(
            ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            )),
        )?;

        ApiAccountDomain::create_sub_account(
            &wallet.address,
            uid,
            &password,
            chain,
            "账户",
            true,
            to_create.len() as u32,
            to_create.to_vec(),
            Some(batch_id.to_string()),
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn init_account(
        uid: &str,
        chain: &str,
        to_init: &[i32],
        batch_id: &str,
    ) -> Result<(), ServiceError> {
        let sn = crate::context::get_context()?.get_sn();
        let mut init_req = ApiAddressInitReq::new().with_batch_id(batch_id);

        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(pool.clone(), uid).await?.ok_or(
            ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            )),
        )?;

        let accounts = ApiAccountRepo::list_by_wallet_address(
            pool.clone(),
            &api_wallet.address,
            None,
            Some(chain),
        )
        .await?;

        for account in accounts {
            if let Ok(map) =
                wallet_utils::address::AccountIndexMap::from_account_id(account.account_id)
            {
                let idx = map.input_index;
                if to_init.contains(&idx) {
                    init_req.address_list.add_address(AddressInitReq::new(
                        uid,
                        &account.address,
                        idx,
                        chain,
                        sn,
                        vec!["".to_string()],
                        &account.name,
                    ));
                }
            }
        }

        if !init_req.address_list.0.is_empty() {
            let data = BackendApiTaskData::new(
                wallet_transport_backend::consts::endpoint::api_wallet::ADDRESS_INIT,
                &init_req,
            )?;
            Tasks::new().push(BackendApiTask::BackendApi(data)).send().await?;
            tracing::info!("recover: 已补发送 init: {:?}", to_init);
        }
        Ok(())
    }

    pub(crate) async fn expand_complete(uid: &str, batch_id: &str) -> Result<(), ServiceError> {
        let pool = crate::context::get_context()?.get_global_sqlite_pool()?;
        let batch = ExpandBatchRepo::get_batch(pool.clone(), batch_id).await?.ok_or(
            ServiceError::Business(crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::account::AccountError::ExpandBatchNotFound
                    .into(),
            )),
        )?;
        let backend = crate::context::get_context()?.get_global_backend_api();
        backend
            .expand_address_complete(ExpandAddressCompleteReq::new(
                uid,
                batch_id,
                &batch.serial_no,
                true,
                None,
            ))
            .await?;
        Ok(())
    }
}
