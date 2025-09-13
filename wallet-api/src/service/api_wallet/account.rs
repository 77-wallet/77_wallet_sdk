use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::{entities::api_wallet::ApiWalletType, repositories::chain::ChainRepo};
use wallet_transport_backend::request::api_wallet::address::{AddressParam, ExpandAddressReq};

use crate::{
    domain::{
        api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
        wallet::WalletDomain,
    },
    messaging::mqtt::topics::api_wallet::AddressAllockType,
};

pub struct ApiAccountService {}

impl ApiAccountService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn expand_address(
        self,
        address_allock_type: AddressAllockType,
        chain_code: &str,
        index: Option<i32>,
        uid: &str,
    ) -> Result<(), crate::ServiceError> {
        ApiWalletDomain::expand_address(&address_allock_type, index, &uid, &chain_code).await?;

        Ok(())
    }

    pub async fn create_account(
        self,
        wallet_address: &str,
        wallet_password: &str,
        // derivation_path: Option<String>,
        indices: Vec<i32>,
        name: &str,
        is_default_name: bool,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let default_chain_list = ChainRepo::get_chain_list(&pool).await?;
        let chains: Vec<String> =
            default_chain_list.iter().map(|chain| chain.chain_code.clone()).collect();
        ApiWalletDomain::create_account(
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
        self,
        address: &str,
        chain_code: &str,
        password: &str,
    ) -> Result<ChainPrivateKey, crate::ServiceError> {
        Ok(ApiAccountDomain::get_private_key(address, chain_code, password).await?)
    }

    pub async fn address_used(
        self,
        chain_code: &str,
        index: i32,
        uid: &str,
    ) -> Result<(), crate::ServiceError> {
        Ok(ApiAccountDomain::address_used(chain_code, index, uid).await?)
    }
}
