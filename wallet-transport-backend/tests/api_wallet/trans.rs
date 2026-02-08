use crate::init;
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::{
    swap::ApiInitSwapReq,
    transaction::{TransAckType, TransEventAckReq, TransType},
};

#[serial_test::serial]
#[tokio::test]
async fn test_trans_event_ack() -> Result<(), wallet_transport_backend::Error> {
    let sn = "b35f7b556b87c87bb1928ea6ab12ef6918b71f5c37fbd53b88e9353ea2093f0b";
    let backend_api = init(sn)?;
    let req =
        ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }
    let req =
        TransEventAckReq::new("CF2003760804267220992", TransType::ColFee, TransAckType::TxRes);
    let res = backend_api.trans_event_ack(&req).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res)?;
    println!("[test_fetch_all_api_tokens] res: {res}");
    Ok(())
}
