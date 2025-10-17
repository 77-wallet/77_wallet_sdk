use crate::init;
use wallet_ecdh::GLOBAL_KEY;

#[tokio::test]
async fn test_api_wallet_chain_list() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;
    let pub_key = r#"-----BEGIN PUBLIC KEY-----
MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEa5VZe6ldctovjscGK1k4Fq/6OMa4C5ke
Fb0OW3wf4gYNiPRKBAU47v77BdGjyT6y7tVeuQ714ql4fhTUVWfnMg==
-----END PUBLIC KEY-----"#;
    GLOBAL_KEY.set_sn("lan48300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa59524");
    GLOBAL_KEY.set_shared_secret(pub_key)?;

    for i in 0..10 {
        let res = backend_api.api_wallet_chain_list("2.0.0").await;
        match res {
            Ok(res) => {
                // let res = serde_json::to_string(&res).unwrap();
                // tracing::info!("[test_api_wallet_chain_list] res: {res:?}");
                // tracing::info!("api_wallet_chain_list: ok");
            }
            Err(e) => {
                // tracing::error!("[test_api_wallet_chain_list] err: {}", e);
                // tracing::error!("api_wallet_chain_list: error: {}", e);
            }
        }
    }
    Ok(())
}
