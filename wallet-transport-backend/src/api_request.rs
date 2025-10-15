use wallet_ecdh::GLOBAL_KEY;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendRequestBody{
    pub key: String,
    pub data: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendRequest {
    pub sn: String,
    pub sign: String,
    pub body: ApiBackendRequestBody,
}

impl ApiBackendRequest {
    pub fn new<T>(req: T) -> Result<Self, crate::Error>
        where T: serde::Serialize
    {
        let req_data = serde_json::json!(req);
        tracing::info!("Request 1: {:?}", req_data);
        let d = GLOBAL_KEY.encrypt(req_data.to_string().as_bytes())?; // base64
        let key = wallet_utils::bytes_to_base64(&d.key);
        let ct = wallet_utils::bytes_to_base64(&d.ciphertext);
        let iv = wallet_utils::bytes_to_base64(&d.nonce);
        tracing::info!("Request encrypt 2, key: {:?}, ct: {:?}, iv: {:?}", key, ct, iv);
        let body = ApiBackendRequestBody {
            key: key.clone(),
            data: ct.clone(),
        };
        tracing::info!("Request encrypt 3 body : {:?}", body);

        // 签名
        let tag = uuid::Uuid::new_v4().to_string();
        let body_data = key + ct.as_str();
        tracing::info!("Request sign 4: {:?}", body_data);
        let vec_sign = GLOBAL_KEY.sign(&tag, body_data.as_bytes())?;
        let sign = wallet_utils::bytes_to_base64(&vec_sign);
        tracing::info!("Request sign 5 sig: {:?}", sign);
        let api_req = ApiBackendRequest{
            sn: GLOBAL_KEY.sn().to_string(),
            sign,
            body,
        };
        let sd = wallet_utils::serde_func::serde_to_string(&api_req).unwrap();
        tracing::info!("Request sign 6 api req: {:?}",  sd);
        Ok(api_req)
    }
}