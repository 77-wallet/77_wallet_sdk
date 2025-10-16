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
    pub fn process<T>(&self, endpoint: &str) -> Result<Option<T>, crate::error::Error>
    where
        T: serde::de::DeserializeOwned + serde::Serialize,
    {
        let tag = uuid::Uuid::new_v4().to_string();
        tracing::info!(
            tag=%tag,
            endpoint = endpoint,
            "Received response: {:?} -------------------------------------",
            self
        );
        // 验证签名
        if self.success {
            if let Some(data) = &self.data {
                let body_data = data.body.key.clone() + data.body.data.as_str();
                tracing::info!(
                    tag=%tag,
                    endpoint = endpoint,
                    "for verify: {:?}", body_data);

                // 验签
                let s = wallet_utils::base64_to_bytes(data.sign.as_str())?;
                GLOBAL_KEY.verify(&tag, body_data.as_bytes(), &s)?;

                // 解密
                let key = wallet_utils::base64_to_bytes(data.body.key.as_str())?;
                let data = wallet_utils::base64_to_bytes(data.body.data.as_str())?;
                let t = GLOBAL_KEY.decrypt(&data, &key)?;
                let res = wallet_utils::serde_func::serde_from_slice(&t)?;
                Ok(res)
            } else {
                Ok(None::<T>)
            }
        } else {
            Err(crate::Error::Backend(self.msg.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        api_response::ApiBackendResponse, response_vo::api_wallet::chain::ApiChainListResp,
    };
    use wallet_ecdh::GLOBAL_KEY;

    #[tokio::test]
    async fn test_response() -> Result<(), crate::Error> {
        wallet_utils::init_test_log();

        let pub_key = r#"-----BEGIN PUBLIC KEY-----
MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEWDZNP0ClbeWJey9hBr2rsjSayQEBywnv
ZXi0RberQCAp+06fOjvr+jZI5qwYGglmMkGJw49tbni6qgm4QNV6WQ==
-----END PUBLIC KEY-----"#;
        GLOBAL_KEY.set_shared_secret(pub_key)?;
        let s = r#"
        {
    "success": true,
    "data": {
        "sign": "T6dBG7LDufmGPgnFxPcxC4ev7QgEW/NDAspkXTAQQDh1ao/lENI8jWtcfCqg4zJ0w+paVTkmcwmemx+u1BjOYg==",
        "body": {
            "key": "Ki02NQIrj5bQ8JLvz8CiRg8ou+VeDAwfx+03cVbEKfw=",
            "data": "aucViXrJLPZ39Z8UEVaP+t6TacrFTSFgt/1kMfBrtYgFNiUPdDHwIlXXcnV5oSjQROyVv0fDmZKCwZH4BlBjC5TQwQDv/H4cH1oOzwyvXrogg1+CMScYkwe1gtMEEuFV+yN3Q+P/w+C7ifuuwp6AXTXXBWFCjYDG5+h7/A3SRHFrvzx/mqe0cKIzvUaKZws7MlQolkISucU80KKEhNyhD6EtL2qTrK8LbVlb3T21TDXdJUvXv0O4XAVqRG2trILYRwiqIn2C1VYRVY8uE9XS/OwDSMknPpjmFB9kQS5PYe8PbCnKu30iGLyGAUPpQU8FE9uu6bq1aUOvDGJA/aSHKDXQ4pxcHelncMUI4dY4s7cBBEjpCgrJT2LXtOPUGVAiDhGWUea2uNWp7gTRQAt45nkmOa/u+oND4unDdLngokztj2htNK5YWBpA40c/gnJHvkaE1EN9k9pAXBqg6ZaRcu41uEIg9SukxWm/3jxGbouoIJ+8h2fUspQ="
        }
    }
}
        "#;
        let res = serde_json::from_str::<ApiBackendResponse>(s).unwrap();
        let x: Option<ApiChainListResp> = res.process("test")?;
        assert!(x.is_some());
        Ok(())
    }
}
