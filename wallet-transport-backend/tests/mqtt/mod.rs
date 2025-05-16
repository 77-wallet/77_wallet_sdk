use wallet_transport_backend::request::{
    MsgConfirmSource, QueryUnconfirmMsgReq, SendMsgConfirm, SendMsgConfirmReq,
};

use crate::init;

#[tokio::test]
async fn test_send_msg_confirm() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = SendMsgConfirmReq {
        list: vec![SendMsgConfirm {
            id: "676059fcab8ff576d42076ef".to_string(),
            source: MsgConfirmSource::Mqtt,
        }],
    };
    let res = backend_api
        .send_msg_confirm(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_send_msg_confirm] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_send_msg_query_unconfirm_msg() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let client_id = "7552bd49a9407eb98164c129d11da7e2".to_string();

    let req = QueryUnconfirmMsgReq { client_id };

    let res = backend_api
        .query_unconfirm_msg(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_send_msg_query_unconfirm_msg] res: {res:?}");

    Ok(())
}
