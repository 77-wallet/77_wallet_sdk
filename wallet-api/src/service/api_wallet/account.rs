use crate::{
    context::Context,
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        wallet::WalletDomain,
    },
    error::service::ServiceError,
    messaging::mqtt::topics::api_wallet::address_allock::AddressAllockType,
    response_vo::api_wallet::account::ApiAccountInfos,
};
use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::{
    entities::api_wallet::ApiWalletType, repositories::api_wallet::chain::ApiChainRepo,
};

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
        index: i32,
        chain_code: Option<String>,
    ) -> Result<ApiAccountInfos, ServiceError> {
        let account_index_map = wallet_utils::address::AccountIndexMap::from_input_index(index)?;
        ApiAccountDomain::list_api_accounts(
            wallet_address,
            Some(account_index_map.account_id),
            chain_code,
        )
        .await
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
}
