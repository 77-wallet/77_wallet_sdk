use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{
        ResourcesRepo, api_account::ApiAccountRepo, api_wallet::ApiWalletRepo,
        chain::ChainRepoTrait as _, coin::CoinRepoTrait as _,
    },
};
use wallet_transport_backend::request::{
    AddressBatchInitReq, TokenQueryPriceReq, api_wallet::address::UploadAllocatedAddressesReq,
};

use crate::{
    domain::{api_wallet::wallet::ApiWalletDomain, chain::ChainDomain, wallet::WalletDomain},
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, CommonTask, task::Tasks},
};

pub struct ApiAccountService {
    pub repo: ResourcesRepo,
}

impl ApiAccountService {
    pub fn new(repo: ResourcesRepo) -> Self {
        Self { repo }
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
        let mut tx = self.repo;

        WalletDomain::validate_password(wallet_password).await?;
        // 根据钱包地址查询是否有钱包

        ApiWalletDomain::create_account(
            tx,
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
}
