use wallet_ecdh::GLOBAL_KEY;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendDataBody {
    pub key: String,
    pub data: String,
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendData {
    pub sign: String,
    pub body: ApiBackendDataBody,
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendResponse {
    pub success: bool,
    pub code: Option<String>,
    pub msg: Option<String>,
    pub data: Option<ApiBackendData>,
}

impl ApiBackendResponse {
    pub fn process<T>(&self) -> Result<T, crate::error::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        tracing::info!("Received response: {:?} -------------------------------------", self);
        // 验证签名
        if self.success {
            if let Some(data) = &self.data {
                let body_data = data.body.key.clone() + data.body.data.as_str();
                // 验签
                let s = wallet_utils::base64_to_bytes(data.sign.as_str())?;
                let pass = GLOBAL_KEY.verify(body_data.as_bytes(), &s)?;
                if pass {
                    // 解密
                    let key = wallet_utils::base64_to_bytes(data.body.key.as_str())?;
                    let data = wallet_utils::base64_to_bytes(data.body.data.as_str())?;
                    let t = GLOBAL_KEY.decrypt(&data, &key)?;
                    let res = wallet_utils::serde_func::serde_from_slice(&t)?;
                    Ok(res)
                } else {
                    Err(crate::error::Error::Backend(Some("verify failed".to_string())))
                }
            } else {
                Err(crate::Error::Backend(Some("data is empty".to_string())))
            }
        } else {
            Err(crate::Error::Backend(self.msg.clone()))
        }
    }
}
