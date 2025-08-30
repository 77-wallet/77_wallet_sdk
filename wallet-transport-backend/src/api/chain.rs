use crate::{
    consts::endpoint::CHAIN_LIST,
    request::{ChainListReq, ChainRpcListReq},
    response::BackendResponse,
    response_vo::chain::{ChainInfos, ChainList},
};

use super::BackendApi;

impl BackendApi {
    pub async fn chain_default_list(&self) -> Result<serde_json::Value, crate::Error> {
        let res = self.client.post("chain/defaultList").send::<BackendResponse>().await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn chain_list(&self, req: ChainListReq) -> Result<ChainList, crate::Error> {
        let res = self.client.post(CHAIN_LIST).json(req).send::<BackendResponse>().await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn _chain_list(&self) -> Result<serde_json::Value, crate::Error> {
        let res = self.client.post("chain/list").send::<BackendResponse>().await?;

        res.process(&self.aes_cbc_cryptor)
    }

    pub async fn chain_rpc_list(&self, req: ChainRpcListReq) -> Result<ChainInfos, crate::Error> {
        let res = self.client.post("chain/rpcList").json(req).send::<BackendResponse>().await?;

        res.process(&self.aes_cbc_cryptor)
    }
}
