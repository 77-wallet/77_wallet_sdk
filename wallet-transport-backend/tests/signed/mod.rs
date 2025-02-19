use wallet_transport_backend::{
    request::{FindAddressRawDataReq, SignedFeeListReq, SignedFindAddressReq},
    SignedCreateOrderReq, SignedUpdateRechargeHashReq, SingedOrderCancelReq,
};

use crate::init;

#[tokio::test]
async fn test_address_find_address_raw_data() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let typ = None;
    let raw_time = None;
    let business_id = Some("218406973127921664".to_string());
    let req = FindAddressRawDataReq::new(None, typ, raw_time, business_id);
    let res = backend_api
        .address_find_address_raw_data(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[test_address_find_address_raw_data] res: {res:#?}");

    Ok(())
}

#[tokio::test]
async fn test_signed_order_create() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let chain_code = "bnb_test";
    let address = "0x5985CE40d3dACf7c1352e464691BC7fb03215928";
    let multisig_address = "0x1C2Ce4352f86D37715EA3a8De1D7122ff8760149";

    let req = SignedCreateOrderReq::new(chain_code, address, multisig_address)
        .with_elements(&1.to_string(), "2");
    let res = backend_api
        .signed_order_create(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[test_signed_order_create] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_cancel_multisig() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = SingedOrderCancelReq {
        order_id: "220236893877571584".to_string(),
        raw_data: "".to_string(),
    };
    let res = backend_api
        .signed_order_cancel(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_cancel_multisig] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_signed_find_address() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = SignedFindAddressReq {
        name: None,
        code: None,
        chain_code: "tron".to_string(),
    };
    let res = backend_api
        .signed_find_address(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[test_signed_find_address] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_signed_fee_list() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = SignedFeeListReq {
        chain_code: "eth".to_string(),
    };
    let res = backend_api
        .signed_fee_list(&aes_cbc_cryptor, req)
        .await
        .unwrap();

    println!("[test_signed_fee_list] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn signed_order_update_signed_hash() -> Result<(), wallet_transport_backend::Error> {
    // let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    // let req = SignedUpdateSignedHashReq::new(
    //     "66ac4377c44f4c2b76932a1f",
    //     "e0cbbf993ccdf05a1f3f620b245673f63407ec6b94447e6b489cbeeb686730ec",
    //     "TL5YGitvEyqUakseGRED2jDUJ8sv6qpLaR",
    // );
    // let res = backend_api.signed_order_update_signed_hash(&aes_cbc_cryptor, req)
    //     .await
    //     .unwrap();

    // println!("[signed_order_update_signed_hash] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn signed_order_update_recharge_hash() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = SignedUpdateRechargeHashReq {
        order_id: "66a1b2da6a5fb47fea0e00fa".to_string(),
        hash: "0ba4f88de631c5218503d37d520e815f40b5d3499b86a7029c15c70e9a379873".to_string(),
        product_code: "1".to_string(),
        receive_chain_code: "tron".to_string(),
        receive_address: "".to_string(),
        raw_data: "".to_string(),
    };
    let res = backend_api
        .signed_order_update_recharge_hash(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[signed_order_update_recharge_hash] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_query_multisig_account() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let id = "214178817818890240";
    let res = backend_api
        .check_multisig_account_is_cancel(&aes_cbc_cryptor, id)
        .await
        .unwrap();

    println!("[test_query_multisig_account] res: {res:?}");

    Ok(())
}
