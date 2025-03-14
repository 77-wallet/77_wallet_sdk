use crate::init;
use wallet_transport_backend::request::AnnouncementListReq;

#[tokio::test]
async fn test_announcement_list() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?;

    let req = AnnouncementListReq {
        // uid: "626e71f65a6eccc7c51f2a8295cae861".to_string(),
        client_id: "6f88a37aca2384cec6029d5983fac0e2".to_string(),
        order_column: "create_time".to_string(),
        page_num: 0,
        page_size: 1000,
    };
    let req_str = wallet_utils::serde_func::serde_to_string(&req).unwrap();
    tracing::info!("req_str: {req_str}");

    let res = backend_api
        .announcement_list(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[test_chain_default_list] res: {res:#?}");
    Ok(())
}
