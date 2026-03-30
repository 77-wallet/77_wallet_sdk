use crate::domain::api_wallet::wallet::ApiWalletDomain;

pub async fn encrypt_seed(password: &str, seed: &[u8]) -> Vec<u8> {
    ApiWalletDomain::encrypt_seed_bundle(password, seed).await.expect("encrypt test seed")
}
