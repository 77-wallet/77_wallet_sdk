use wallet_transport_backend::api::BackendApi;

mod address;
mod announcement;
mod app;
mod chain;
mod coin;
mod config;
mod device;
mod mqtt;
mod signed;
mod stake;
mod transaction;

pub fn init(
) -> Result<(wallet_utils::cbc::AesCbcCryptor, BackendApi), wallet_transport_backend::Error> {
    wallet_utils::init_test_log();
    let base_url = "https://xxxxxxx";
    Ok((
        create_aes_cryptor(),
        BackendApi::new(Some(base_url.to_string()), None)?,
    ))
}

pub(crate) fn create_aes_cryptor() -> wallet_utils::cbc::AesCbcCryptor {
    wallet_utils::cbc::AesCbcCryptor::new("1234567890123456", "1234567890123456")
}
