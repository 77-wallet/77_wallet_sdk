use std::collections::HashMap;
use wallet_transport_backend::api::BackendApi;

mod api_wallet;
mod wallet;

pub fn init() -> Result<BackendApi, wallet_transport_backend::Error> {
    wallet_utils::init_test_log();
    let base_url = "https://walletapi.puke668.top";

    let client_id = "";
    let sn = "lan48300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa59524";
    let mut headers_opt : HashMap<String, String> = HashMap::new();
    // headers_opt.insert("clientId".to_string(), client_id.clone());
    headers_opt.insert("AW-SEC-ID".to_string(), sn.to_string());
    Ok(BackendApi::new(Some(base_url.to_string()), Some(headers_opt), create_aes_cryptor())?)
}

pub(crate) fn create_aes_cryptor() -> wallet_utils::cbc::AesCbcCryptor {
    wallet_utils::cbc::AesCbcCryptor::new("u3es1w0suq515aiw", "0000000000000000")
}
