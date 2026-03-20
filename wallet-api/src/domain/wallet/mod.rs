use wallet_types::chain::{
    address::r#type::{
        AddressType, BTC_ADDRESS_TYPES, DOG_ADDRESS_TYPES, LTC_ADDRESS_TYPES, TON_ADDRESS_TYPES,
    },
    chain::ChainCode,
};

use crate::{application::wallet::WalletApplication, error::service::ServiceError};

const DEFAULT_SALT: &str = "salt";

pub struct WalletDomain {}

impl Default for WalletDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletDomain {
    pub fn new() -> Self {
        Self {}
    }

    pub(crate) fn encrypt_password(password: &str, salt: &str) -> Result<String, ServiceError> {
        let encrypted_password = wallet_utils::pbkdf2_string(
            password,
            &format!("{}{}", salt, DEFAULT_SALT),
            100000,
            32,
        )?;
        Ok(encrypted_password)
    }

    pub fn address_type_by_chain(chian: ChainCode) -> Vec<AddressType> {
        match chian {
            ChainCode::Bitcoin => BTC_ADDRESS_TYPES.to_vec(),
            ChainCode::Dogcoin => DOG_ADDRESS_TYPES.to_vec(),
            ChainCode::Litecoin => LTC_ADDRESS_TYPES.to_vec(),
            ChainCode::Ton => TON_ADDRESS_TYPES.to_vec(),
            _ => vec![AddressType::Other],
        }
    }

    pub(crate) async fn validate_password(password: &str) -> Result<(), ServiceError> {
        WalletApplication::validate_password(password).await
    }

    pub(crate) async fn upgrade_algorithm(password: &str) -> Result<(), ServiceError> {
        WalletApplication::upgrade_algorithm(password).await
    }

    pub(crate) async fn get_seed(
        dirs: &crate::dirs::Dirs,
        wallet_address: &str,
        wallet_password: &str,
    ) -> Result<Vec<u8>, ServiceError> {
        WalletApplication::get_seed(dirs, wallet_address, wallet_password).await
    }

    pub(crate) async fn restart_existing_wallet(
        &self,
        core_pool: wallet_database::CoreDbPool,
        address: &str,
    ) -> Result<std::collections::HashSet<u32>, ServiceError> {
        WalletApplication::restart_existing_wallet(core_pool, address).await
    }

    pub(crate) async fn check_api_wallet_exist(address: &str) -> Result<bool, ServiceError> {
        WalletApplication::check_api_wallet_exist(address).await
    }

    pub(crate) async fn generate_password_proof(password: &str) -> Result<String, ServiceError> {
        WalletApplication::generate_password_proof(password).await
    }
}
