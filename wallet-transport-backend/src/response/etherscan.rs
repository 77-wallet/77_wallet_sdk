#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct EtherscanResponse<T> {
    pub status: String,
    pub message: String,
    pub result: T,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Data {
    pub items: serde_json::Value,
    pub module: Option<serde_json::Value>,
    pub success: bool,
    pub message: Option<String>,
}

impl Data {
    pub fn serde<T: for<'de> serde::Deserialize<'de>>(self) -> Result<T, crate::Error> {
        if self.success {
            if let Some(module) = self.module {
                Ok(wallet_utils::serde_func::serde_from_value(module)?)
            } else {
                Err(crate::Error::Backend(self.message))
            }
        } else {
            Err(crate::Error::Backend(self.message))
        }
    }
}

impl<T> super::BackendRespExt for EtherscanResponse<T> {
    fn code(&self) -> Option<i64> {
        // etherscan 没明确 code，可用 message 映射
        if self.message.to_lowercase().contains("rate limit") {
            Some(429)
        } else {
            None
        }
    }

    fn success(&self) -> bool {
        self.status == "1"
    }

    fn message(&self) -> Option<&str> {
        Some(self.message.as_str())
    }
}
