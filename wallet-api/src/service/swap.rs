use crate::response_vo::swap::{SupportChain, SwapTokenInfo};

pub struct SwapServer {
    // pub client: HttpClient,
}

impl SwapServer {
    pub fn new() -> Self {
        Self {}
    }
}

impl SwapServer {
    pub async fn quote(&self) -> Result<(), crate::ServiceError> {
        // 查询后端,获取报价
        // 模拟执行获得手续费
        Ok(())
    }

    pub async fn token_list(&self) -> Result<Vec<SwapTokenInfo>, crate::ServiceError> {
        // self.client.post()

        let data = SwapTokenInfo {
            name: String::from("usdt"),
            symbol: String::from("USDT"),
            decimals: 0,
            chain_code: String::from("tron"),
            contract_address: String::from("xxx"),
        };

        Ok(vec![data])
    }

    pub async fn chain_list(&self) -> Result<Vec<SupportChain>, crate::ServiceError> {
        let data = SupportChain {
            name: String::from("tron"),
            chain_code: String::from("tron"),
        };

        Ok(vec![data])
    }

    pub async fn swap(&self) {}
}
