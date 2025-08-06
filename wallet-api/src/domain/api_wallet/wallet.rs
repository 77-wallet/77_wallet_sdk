use wallet_crypto::{
    EncryptedJsonDecryptor as _, EncryptedJsonGenerator as _, KeystoreJsonDecryptor,
    KeystoreJsonGenerator,
};
use wallet_database::{
    entities::api_wallet::ApiWalletType,
    repositories::{api_wallet::ApiWalletRepo, wallet::WalletRepo},
};

pub struct ApiWalletDomain {}

impl ApiWalletDomain {
    pub(crate) async fn upsert_api_wallet(
        uid: &str,
        wallet_name: &str,
        wallet_address: &str,
        password: &str,
        phrase: &str,
        seed: &[u8],
        algorithm: wallet_crypto::KdfAlgorithm,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let phrase = wallet_utils::serde_func::serde_to_vec(&phrase)?;

        let rng = rand::thread_rng();
        let mut generator = KeystoreJsonGenerator::new(rng.clone(), algorithm.clone());
        let phrase = generator.generate(password.as_bytes(), &phrase)?;
        let phrase = wallet_utils::serde_func::serde_to_string(&phrase)?;
        let seed =
            KeystoreJsonGenerator::new(rng, algorithm).generate(password.as_bytes(), seed)?;
        let seed = wallet_utils::serde_func::serde_to_string(&seed)?;

        ApiWalletRepo::upsert(
            &pool,
            &uid,
            wallet_name,
            wallet_address,
            &phrase,
            &seed,
            api_wallet_type,
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn decrypt_seed(
        password: &str,
        seed: &str,
    ) -> Result<Vec<u8>, crate::ServiceError> {
        let data = KeystoreJsonDecryptor.decrypt(password.as_ref(), seed)?;
        Ok(data)
    }

    pub(crate) async fn check_normal_wallet_exist(
        address: &str,
    ) -> Result<bool, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        Ok(WalletRepo::detail(&pool, address).await?.is_some())
    }

    pub(crate) async fn unbind_uid(uid: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, uid, ApiWalletType::SubAccount)
            .await?
            .ok_or(crate::BusinessError::ApiWallet(
                crate::ApiWalletError::NotFound,
            ))?;
        ApiWalletRepo::upbind_uid(&pool, &api_wallet.address, ApiWalletType::SubAccount).await?;

        Ok(())
    }
}
