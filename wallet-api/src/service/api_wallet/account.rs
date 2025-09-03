use wallet_chain_interact::types::ChainPrivateKey;
use wallet_database::entities::api_wallet::ApiWalletType;
use wallet_transport_backend::request::api_wallet::address::UploadAllocatedAddressesReq;

use crate::domain::{
    api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    wallet::WalletDomain,
};

pub struct ApiAccountService {}

impl ApiAccountService {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn upload_allocated_addresses(
        self,
        wallet_address: &str,
        addresses: Vec<String>,
    ) -> Result<(), crate::ServiceError> {
        let backend_api = crate::Context::get_global_backend_api()?;

        let req = UploadAllocatedAddressesReq::new(wallet_address, addresses);
        backend_api.upload_allocated_addresses(&req).await?;

        Ok(())
    }

    pub async fn create_account(
        self,
        wallet_address: &str,
        wallet_password: &str,
        // derivation_path: Option<String>,
        index: Option<i32>,
        name: &str,
        is_default_name: bool,
        number: u32,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包

        ApiWalletDomain::create_account(
            wallet_address,
            wallet_password,
            index,
            name,
            is_default_name,
            number,
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
}
