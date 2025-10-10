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
        let d = GLOBAL_KEY.encrypt(req_data.to_string().as_bytes())?; // base64
        let body = ApiBackendRequestBody {
            key: wallet_utils::bytes_to_base64(&d.key),
            data: wallet_utils::bytes_to_base64(&d.ciphertext),
        };

        let body_data = serde_json::json!(body);
        // 签名
        let sig = GLOBAL_KEY.sign(body_data.to_string().as_bytes())?;
        let api_req = ApiBackendRequest{
            sn: GLOBAL_KEY.sn().to_string(),
            sign: wallet_utils::bytes_to_base64(&sig),
            body,
        };
        Ok(api_req)
    }
}