use wallet_transport_backend::request::{
    DeviceBindAddressReq, DeviceDeleteReq, DeviceInitReq, DeviceUnbindAddress, KeysInitReq,
    UpdateAppIdReq,
};

use crate::init;

#[tokio::test]
async fn test_device_init() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = DeviceInitReq {
        device_type: "ANDROID".to_string(),
        sn: "dead3430844c05f837d2301d7b3bc2f3".to_string(),
        code: "3".to_string(),
        system_ver: "4".to_string(),
        iemi: Some("5".to_string()),
        meid: Some("6".to_string()),
        iccid: Some("7".to_string()),
        mem: Some("8".to_string()),
        // invitee: false,
    };
    let res = backend_api
        .device_init(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_device_init] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_min_config() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let res = backend_api
        .fetch_min_config(&aes_cbc_cryptor, "guangxiang".to_string())
        .await
        .unwrap();

    println!("[test_min_config] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_device_delete() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = DeviceDeleteReq {
        sn: "9580b55ec4a1d3d3af85077ae0c4c901885b1123e50f830cbd5bfbbe0cb161a3".to_string(),
        uid_list: vec!["c7c8453fbf4368279c822a1c39f3c955".to_string()],
    };
    let res = backend_api
        .device_delete(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_device_delete] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_device_bind_address() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = DeviceBindAddressReq {
        sn: "14ae14461d0891116eb85ef447ecb28dc22781d987b5cb0f75f8d3bcca18ebed".to_string(),
        address: vec![DeviceUnbindAddress {
            chain_code: "tron".to_string(),
            address: "TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB".to_string(),
        }],
    };
    let res = backend_api
        .device_bind_address(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_device_bind_address] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_device_unbind_address() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = DeviceBindAddressReq {
        sn: "bdb6412a9cb4b12c48ebe1ef4e9f052b07af519b7485cd38a95f38d89df97cb8".to_string(),
        address: vec![DeviceUnbindAddress {
            chain_code: "tron".to_string(),
            address: "TFzMRRzQFhY9XFS37veoswLRuWLNtbyhiB".to_string(),
        }],
    };
    let res = backend_api
        .device_unbind_address(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_device_unbind_address] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_keys_init() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = KeysInitReq {
        uid: "dead3430844c05f837d2301d7b3bc2f3".to_string(),
        sn: "dead3430844c05f837d2301d7b3bc2f3".to_string(),
        client_id: Some("guanxiang".to_string()),
        device_type: Some("ANDROID".to_string()),
        name: "asad".to_string(),
        invite_code: "".to_string(),
    };
    let res = backend_api.keys_init(&aes_cbc_cryptor, &req).await.unwrap();

    println!("[test_keys_init] res: {res:?}");

    Ok(())
}

#[tokio::test]
async fn test_update_app_id() -> Result<(), wallet_transport_backend::Error> {
    let (aes_cbc_cryptor, backend_api) = init()?; // Initialize the cryptor and API

    let req = UpdateAppIdReq {
        sn: "dead3430844c05f837d2301d7b3bc2f3".to_string(),
        app_id: "asad".to_string(),
    };
    let res = backend_api
        .update_app_id(&aes_cbc_cryptor, &req)
        .await
        .unwrap();

    println!("[test_update_app_id] res: {res:?}");

    Ok(())
}
#[tokio::test]
async fn test_main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder().build()?;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);
    headers.insert("Cookie", "sl-session=/hUSdIIxfWY+3tUC5NQC9A==".parse()?);

    let data = r#"{
        "deviceType": "ANDROID",
        "sn": "2",
        "code": "2",
        "systemVer": "3",
        "iemi": "4",
        "meid": "5",
        "iccid": "6",
        "mem": "7"
    }"#;

    let json: serde_json::Value = serde_json::from_str(&data)?;

    let request = client
        .request(reqwest::Method::POST, "http://api.wallet.net/device/init")
        .headers(headers)
        .json(&json);

    let response = request.send().await?;
    let body = response.text().await?;

    println!("{}", body);

    Ok(())
}
