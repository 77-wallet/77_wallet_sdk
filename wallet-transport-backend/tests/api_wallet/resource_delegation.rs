use crate::init;
use wallet_ecdh::GLOBAL_KEY;
use wallet_transport_backend::request::api_wallet::{
    resource_delegation::ResourceDelegationApplyReq, swap::ApiInitSwapReq,
};

#[serial_test::serial]
#[tokio::test]
async fn test_apply_resource_delegation() -> Result<(), wallet_transport_backend::Error> {
    let sn = "666";
    let backend_api = init(sn)?;
    let req = ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let req = ResourceDelegationApplyReq::new(
        "test-uid",
        "rsc_delegate_C2036586295360655360",
        "C2036586295360655360",
        "tron",
        "T_address_needs_energy",
        1,
        "100000",
        5,
    );
    let res = backend_api.apply_resource_delegation(&req).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res)?;
    println!("[test_apply_resource_delegation] res: {res}");
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn test_apply_resource_delegation_withdraw() -> Result<(), wallet_transport_backend::Error> {
    let sn = "666";
    let backend_api = init(sn)?;
    let req = ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let req = ResourceDelegationApplyReq::new(
        "test-uid",
        "rsc_delegate_W2036586295360655361",
        "W2036586295360655361",
        "tron",
        "T_withdraw_address",
        1,
        "50000",
        7,
    );
    let res = backend_api.apply_resource_delegation(&req).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res)?;
    println!("[test_apply_resource_delegation_withdraw] res: {res}");
    Ok(())
}

#[serial_test::serial]
#[tokio::test]
async fn test_apply_resource_delegation_bandwidth() -> Result<(), wallet_transport_backend::Error> {
    let sn = "666";
    let backend_api = init(sn)?;
    let req = ApiInitSwapReq { sn: sn.to_string(), client_pub_key: GLOBAL_KEY.secret_pub_key() };
    let res = backend_api.init_swap(&req).await?;
    if let Some(data) = res.data {
        GLOBAL_KEY.set_shared_secret(&data.pub_key)?;
    }

    let req = ResourceDelegationApplyReq::new(
        "test-uid",
        "rsc_delegate_C2036586295360655362",
        "C2036586295360655362",
        "tron",
        "T_address_needs_bandwidth",
        2,
        "200000",
        5,
    );
    let res = backend_api.apply_resource_delegation(&req).await?;
    let res = wallet_utils::serde_func::serde_to_string(&res)?;
    println!("[test_apply_resource_delegation_bandwidth] res: {res}");
    Ok(())
}
