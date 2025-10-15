use wallet_transport_backend::api::BackendApi;

mod api_wallet;
mod wallet;

pub fn init() -> Result<BackendApi, wallet_transport_backend::Error> {
    wallet_utils::init_test_log();
    let base_url = "https://test-api.puke668.top";
    Ok(BackendApi::new(Some(base_url.to_string()), None, create_aes_cryptor())?)
}

pub(crate) fn create_aes_cryptor() -> wallet_utils::cbc::AesCbcCryptor {
    wallet_utils::cbc::AesCbcCryptor::new("u3es1w0suq515aiw", "0000000000000000")
}
