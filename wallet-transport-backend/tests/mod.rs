use std::collections::HashMap;

use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::api::BackendApi;

mod api_wallet;
mod wallet;

pub fn init() -> Result<BackendApi, wallet_transport_backend::Error> {
    //     let pub_key = r#"-----BEGIN PUBLIC KEY-----
    // MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEvuj2vgg8mlTp4Ex8IkKk7Q/vYgHfazxi
    // dTva9NSNj/C1EYbx9Yy+126BjSomU9JSLI57RPIhhBFVx8zu/v6k2g==
    // -----END PUBLIC KEY-----"#;
    // GLOBAL_KEY.set_shared_secret(pub_key)?;
    GLOBAL_KEY.set_sn("wenjing");

    wallet_utils::init_test_log();
    // let base_url = "https://test-api.puke668.top";
    let base_url = "https://walletapi.puke668.top";

    let mut headers_opt = HashMap::new();
    headers_opt.insert("clientId".to_string(), "5bc38769533b4ef6d209bb501b199ca0".to_string());
    headers_opt.insert("AW-SEC-ID".to_string(), "wenjing".to_string());

    let backend_api =
        BackendApi::new(Some(base_url.to_string()), Some(headers_opt), create_aes_cryptor())?;

    Ok(backend_api)
}

pub(crate) fn create_aes_cryptor() -> wallet_utils::cbc::AesCbcCryptor {
    wallet_utils::cbc::AesCbcCryptor::new("u3es1w0suq515aiw", "0000000000000000")
}
