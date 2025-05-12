pub mod config;
pub mod mqtt;
use config::ConfigDomain;
use wallet_crypto::KdfAlgorithm;
use wallet_database::{
    entities::config::config_key::{KEYSTORE_KDF_ALGORITHM, WALLET_TREE_STRATEGY},
    factory::RepositoryFactory,
    repositories::device::DeviceRepoTrait,
};

use crate::{infrastructure::task_queue::BackendApiTaskData, service::device::APP_ID};

pub struct AppDomain<T> {
    phantom: std::marker::PhantomData<T>,
}
impl<T> Default for AppDomain<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> AppDomain<T> {
    pub fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
        }
    }
}

pub struct DeviceDomain;

impl DeviceDomain {
    pub fn device_content(
        device: &wallet_database::entities::device::DeviceEntity,
    ) -> Result<String, crate::ServiceError> {
        let identifier = DeviceDomain::device_identifier(&device.sn, &device.device_type);

        Ok(wallet_utils::ecb::Aes128EcbCryptor::new(APP_ID)
            .unwrap()
            .encrypt(&identifier)
            .unwrap())
    }

    pub fn md5_sn(sn: &str) -> String {
        wallet_utils::md5(sn)
    }

    pub fn client_device_by_sn(sn: &str, device_type: &str) -> String {
        let identifier = DeviceDomain::device_identifier(sn, device_type);
        DeviceDomain::client_id_by_identifier(&identifier)
    }

    //  设备的唯一标识:(app_id,sn,device_type)
    pub fn device_identifier(sn: &str, device_type: &str) -> String {
        format!("{}_{}_{}", APP_ID, sn, device_type)
    }

    // 根据设备唯一标识计算：client_id
    pub fn client_id_by_identifier(identifier: &str) -> String {
        wallet_utils::md5(identifier)
    }

    pub fn client_id_by_device(
        device: &wallet_database::entities::device::DeviceEntity,
    ) -> Result<String, crate::ServiceError> {
        let identifier = DeviceDomain::device_identifier(&device.sn, &device.device_type);

        Ok(DeviceDomain::client_id_by_identifier(&identifier))
    }

    pub(crate) async fn gen_device_bind_address_task_data(
    ) -> Result<BackendApiTaskData, crate::ServiceError> {
        let device_bind_address_task_data = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::DEVICE_BIND_ADDRESS,
            &(),
        )?;
        Ok(device_bind_address_task_data)
    }

    pub(crate) async fn gen_device_unbind_all_address_task_data(
        accounts: &[wallet_database::entities::account::AccountEntity],
        multisig_accounts: Vec<wallet_database::entities::multisig_account::MultisigAccountEntity>,
        sn: &str,
    ) -> Result<BackendApiTaskData, crate::ServiceError> {
        let mut device_unbind_address_req =
            wallet_transport_backend::request::DeviceBindAddressReq::new(sn);
        for account in accounts {
            device_unbind_address_req.push(&account.chain_code, &account.address);
        }
        for multisig_account in multisig_accounts {
            device_unbind_address_req.push(&multisig_account.chain_code, &multisig_account.address);
        }
        let device_unbind_address_task = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::DEVICE_UNBIND_ADDRESS,
            &device_unbind_address_req,
        )?;
        Ok(device_unbind_address_task)
    }

    pub(crate) async fn check_wallet_password_is_null() -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let mut repo = RepositoryFactory::repo(pool.clone());

        let (keystore_kdf_algorithm, wallet_tree_strategy) = if let Some(device) =
            DeviceRepoTrait::get_device_info(&mut repo).await?
            && device.password.is_some()
        {
            let keystore_kdf_algorithm = wallet_database::entities::config::KeystoreKdfAlgorithm {
                keystore_kdf_algorithm: KdfAlgorithm::Scrypt,
            };
            let wallet_tree_strategy = wallet_database::entities::config::WalletTreeStrategy {
                wallet_tree_strategy: wallet_tree::WalletTreeStrategy::V1,
            };
            (keystore_kdf_algorithm, wallet_tree_strategy)
        } else {
            let keystore_kdf_algorithm = wallet_database::entities::config::KeystoreKdfAlgorithm {
                keystore_kdf_algorithm: KdfAlgorithm::Argon2id,
            };
            let wallet_tree_strategy = wallet_database::entities::config::WalletTreeStrategy {
                wallet_tree_strategy: wallet_tree::WalletTreeStrategy::V2,
            };
            (keystore_kdf_algorithm, wallet_tree_strategy)
        };

        ConfigDomain::set_config(
            KEYSTORE_KDF_ALGORITHM,
            &keystore_kdf_algorithm.to_json_str()?,
        )
        .await?;
        ConfigDomain::set_config(WALLET_TREE_STRATEGY, &wallet_tree_strategy.to_json_str()?)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_identifier() {
        let identifier = DeviceDomain::device_identifier(
            // "14ae14461d0891116eb85ef447ecb28dc22781d987b5cb0f75f8d3bcca18ebed",
            "5bb0eada7cb7290b5d196362e6def48dcb9703e1468c0fb28eb7dd61073875e6",
            "IOS",
        );

        let client_id = DeviceDomain::client_id_by_identifier(&identifier);

        println!("identifier:{},client_id:{}", identifier, client_id);
    }
}
