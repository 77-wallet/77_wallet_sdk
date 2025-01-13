use crate::{
    response::BackendResponse,
    response_vo::address::{AddressDetailsList, AssertResp},
};

use super::BackendApi;

impl BackendApi {
    pub async fn address_init(
        &self,
        req: &crate::request::AddressInitReq,
    ) -> Result<Option<()>, crate::Error> {
        let res = self
            .client
            .post("address/init")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    pub async fn address_find_multisiged_details(
        &self,
        req: crate::request::AddressDetailsReq,
    ) -> Result<AddressDetailsList, crate::Error> {
        let res = self
            .client
            .post("/address/findMultiSignedDetails")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    // 地址的资产 uid--> 钱包uid,
    pub async fn wallet_assets_list(
        &self,
        uid: String,
        address: Option<String>,
    ) -> Result<AssertResp, crate::Error> {
        let req = serde_json::json!({
            "uid":uid,
            "address":address,
        });

        let res = self
            .client
            .post("wallet/assets/list")
            .json(req)
            .send::<BackendResponse>()
            .await?;

        res.process()
    }
}

#[cfg(test)]
mod test {
    use crate::{
        api::BackendApi,
        request::{AddressDetailsReq, AddressInitReq},
    };
    use wallet_utils::init_test_log;

    #[tokio::test]
    async fn test_address_init() {
        init_test_log();
        let base_url = crate::consts::BASE_URL;

        let uid = "cd2ac48fa33ba24a8bc0d89e7658a2cd";
        let req = AddressInitReq {
            uid: uid.to_string(),
            address: "TLzteCJi4jSGor5EDRYZcdQ4hsZRQQZ4XR".to_string(),
            index: 0,
            chain_code: "tron".to_string(),
            sn: "3f76bd432e027aa97d11f2c3f5092bee195991be461486f0466eec9d46940e9e".to_string(),
            contract_address: vec!["".to_string()],
            name: "test".to_string(),
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .address_init(&req)
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_address_details() {
        init_test_log();
        let base_url = crate::consts::BASE_URL;

        let req = AddressDetailsReq {
            // address: "TSL4wp6qcLwub88FmEu2gozA1Buz8CnsTn".to_string(),
            // address: "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string(),
            // address: "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N".to_string(),
            address: "TAU1t14o8zZksWRKjwqAVPTMXczUZzvMR1".to_string(),
            chain_code: "tron".to_string(),
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .address_find_multisiged_details(req)
            .await
            .unwrap();

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_assests_list() {
        init_test_log();
        let base_url = "https://test-api.puke668.top";

        let uid = "074209f318e1079c7910c336df5745c57d31da251ebecd7cfda6d13206b71699".to_string();
        let address = None;

        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .wallet_assets_list(uid, address)
            .await;

        println!(" res: {res:?}");
    }
}
