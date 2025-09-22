use crate::init;
use wallet_transport_backend::request::{AddressInitReq, api_wallet::address::ApiAddressInitReq};

#[tokio::test]
async fn test_expand_address() -> Result<(), wallet_transport_backend::Error> {
    let backend_api = init()?;

    let uid = "eb7a5f6ce1234b0d9de0d63750d6aa2c1661e89a3cc9c1beb23aad3bd324071c";
    let mut req = ApiAddressInitReq::new();
    let address_param = AddressInitReq::new(
        uid,
        "TMao3zPmTqNJWg3ZvQtXQxyW1MuYevTMHt",
        1,
        "tron",
        "1",
        vec![],
        "test",
    );
    req.address_list.add_address(address_param);

    let res = backend_api.expand_address(&req).await.unwrap();

    println!("[test_chain_default_list] res: {res:?}");
    Ok(())
}
