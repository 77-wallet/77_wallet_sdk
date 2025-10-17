use crate::{
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        wallet::WalletDomain,
    },
    error::service::ServiceError,
    infrastructure::task_queue::{
        backend::{BackendApiTask, BackendApiTaskData},
        task::Tasks,
    },
    messaging::mqtt::topics::api_wallet::cmd::address_allock::AddressAllockType,
    response_vo::api_wallet::account::ApiAccountInfos,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::{
    entities::{api_account::ApiAccountEntity, api_wallet::ApiWalletType},
    repositories::{
        api_wallet::{account::ApiAccountRepo, chain::ApiChainRepo, wallet::ApiWalletRepo},
        wallet::WalletRepo,
    },
};
use wallet_transport_backend::request::AddressUpdateAccountNameReq;

pub struct ApiAccountService {
    ctx: &'static Context,
}

impl ApiAccountService {
    pub fn new(ctx: &'static Context) -> Self {
        Self { ctx }
    }

    pub async fn list_api_accounts(
        &self,
        wallet_address: &str,
        account_id: Option<u32>,
        chain_code: Option<String>,
    ) -> Result<ApiAccountInfos, ServiceError> {
        ApiAccountDomain::list_api_accounts(wallet_address, account_id, chain_code).await
    }

    pub async fn expand_address(
        &self,
        address_allock_type: AddressAllockType,
        chain_code: &str,
        index: Option<i32>,
        uid: &str,
        number: u32,
        serial_no: &str,
    ) -> Result<(), ServiceError> {
        ApiWalletDomain::expand_address(
            &address_allock_type,
            index,
            &uid,
            &chain_code,
            number,
            serial_no,
        )
        .await?;

        Ok(())
    }

    pub async fn create_account(
        &self,
        wallet_address: &str,
        wallet_password: &str,
        // derivation_path: Option<String>,
        indices: Vec<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let pool = self.ctx.get_global_sqlite_pool()?;
        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;
        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        ApiAccountDomain::create_api_account(
            wallet_address,
            wallet_password,
            chains,
            indices,
            name,
            is_default_name,
            api_wallet_type,
        )
        .await?;

        Ok(())
    }

    pub async fn create_withdrawal_account(
        &self,
        wallet_address: &str,
        wallet_password: &str,
        derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
    ) -> Result<(), crate::error::service::ServiceError> {
        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let pool = self.ctx.get_global_sqlite_pool()?;
        let default_chain_list = ApiChainRepo::get_chain_list(&pool).await?;

        let api_wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::ApiWalletError::NotFound,
            ),
        )?;
        // 根据派生路径
        let hd_path = if let Some(derivation_path) = &derivation_path {
            let hd_path = wallet_chain_instance::derivation_path::get_account_hd_path_from_path(
                derivation_path,
            )?;
            Some(hd_path)
        } else {
            None
        };

        // 如果有指定派生路径，就获取该链的所有chain_code
        let chains: Vec<String> = if let Some(hd_path) = &hd_path {
            hd_path.get_chain_codes()?.0.into_iter().map(|path| path.to_string()).collect()
        }
        // 或者使用默认链的链码
        else {
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect()
        };

        // 获取该账户的最大索引，并加一
        let account_index_map = if let Some(index) = index {
            let index = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
            if ApiAccountRepo::has_account_id(
                &pool,
                &api_wallet.address,
                index.account_id,
                ApiWalletType::Withdrawal,
            )
            .await?
            {
                return Err(crate::error::service::ServiceError::Business(
                    crate::error::business::BusinessError::Account(
                        crate::error::business::account::AccountError::AlreadyExist,
                    ),
                ));
            };
            index
        } else if let Some(hd_path) = hd_path {
            wallet_utils::address::AccountIndexMap::from_index(hd_path.get_account_id()?)?
        } else if let Some(max_account) =
            ApiAccountRepo::account_detail_by_max_id_and_wallet_address(
                &pool,
                &api_wallet.address,
                ApiWalletType::Withdrawal,
            )
            .await?
        {
            wallet_utils::address::AccountIndexMap::from_account_id(max_account.account_id + 1)?
        } else {
            wallet_utils::address::AccountIndexMap::from_account_id(1)?
        };

        ApiAccountDomain::create_api_account(
            wallet_address,
            wallet_password,
            chains,
            vec![account_index_map.input_index],
            name,
            is_default_name,
            ApiWalletType::Withdrawal,
        )
        .await?;

        Ok(())
    }

    pub async fn get_account_private_key(
        &self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, crate::error::service::ServiceError> {
        Ok(ApiAccountDomain::get_private_key(address, chain_code, password).await?)
    }

    pub async fn address_used(
        &self,
        chain_code: &str,
        index: i32,
        uid: &str,
    ) -> Result<(), ServiceError> {
        Ok(ApiAccountDomain::address_used(chain_code, index, uid).await?)
    }

    pub async fn get_account_list(
        &self,
        wallet_address: Option<&str>,
        account_id: Option<u32>,
    ) -> Result<Vec<ApiAccountEntity>, crate::error::service::ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        Ok(ApiAccountRepo::list_by_wallet_address_account_id(&pool, wallet_address, account_id)
            .await?)
    }

    pub async fn edit_account_name(
        &self,
        account_id: u32,
        wallet_address: &str,
        name: &str,
    ) -> Result<(), ServiceError> {
        let pool = self.ctx.get_global_sqlite_pool()?;
        let accounts =
            ApiAccountRepo::edit_account_name(&pool, wallet_address, account_id, name).await?;

        let wallet = ApiWalletRepo::find_by_address(&pool, wallet_address).await?;
        if wallet.is_none() {
            return Err(crate::error::business::BusinessError::Wallet(
                crate::error::business::wallet::WalletError::NotFound,
            )
            .into());
        }
        let wallet = wallet.unwrap();

        let account_index_map =
            wallet_utils::address::AccountIndexMap::from_account_id(account_id)?;

        let req =
            AddressUpdateAccountNameReq::new(&wallet.uid, account_index_map.input_index, name);
        let req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::ADDRESS_UPDATE_ACCOUNT_NAME,
            &req,
        )?;
        Tasks::new().push(BackendApiTask::BackendApi(req)).send().await?;

        Ok(())
    }
}
