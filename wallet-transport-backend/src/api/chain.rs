use crate::{
    request::ChainRpcListReq,
    response::BackendResponse,
    response_vo::chain::{ChainInfos, ChainList},
};

use super::BackendApi;

impl BackendApi {
    pub async fn chain_default_list(&self) -> Result<serde_json::Value, crate::Error> {
        let res = self
            .client
            .post("chain/defaultList")
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    pub async fn chain_list(&self) -> Result<ChainList, crate::Error> {
        let res = self
            .client
            .post("chain/list")
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    pub async fn _chain_list(&self) -> Result<serde_json::Value, crate::Error> {
        let res = self
            .client
            .post("chain/list")
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    pub async fn chain_rpc_list(&self, req: ChainRpcListReq) -> Result<ChainInfos, crate::Error> {
        let res = self
            .client
            .post("chain/rpcList")
            .json(req)
            .send::<BackendResponse>()
            .await?;

        res.process()
    }
}

#[cfg(test)]
mod test {

    use wallet_utils::init_test_log;

    use crate::{api::BackendApi, request::ChainRpcListReq};

    #[tokio::test]
    async fn test_chain_default_list() {
        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .chain_default_list()
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn _chain_list() {
        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            ._chain_list()
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_chain_list() {
        init_test_log();
        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .chain_list()
            .await
            .unwrap();

        tracing::info!("[test_chain_list] res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("[test_chain_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_chain_rpc_list() {
        init_test_log();
        let base_url = crate::consts::BASE_URL;

        let chain_code = vec!["eth".to_string(), "tron".to_string(), "btc".to_string()];
        let req = ChainRpcListReq::new(chain_code);
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .chain_rpc_list(req)
            .await
            .unwrap();

        tracing::info!("[test_chain_list] res: {res:?}");
        let res = wallet_utils::serde_func::serde_to_string(&res).unwrap();
        tracing::info!("[test_chain_list] res: {res:?}");
    }
}
