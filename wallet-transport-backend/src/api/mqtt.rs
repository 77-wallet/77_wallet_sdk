use super::BackendApi;
use crate::response::BackendResponse;

impl BackendApi {
    pub async fn send_msg_confirm(
        &self,
        req: &crate::request::SendMsgConfirmReq,
    ) -> Result<(), crate::Error> {
        let res = self
            .client
            .post("sendMsg/confirm")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process()
    }

    pub async fn send_msg_query_unconfirm_msg(
        &self,
        req: &crate::request::SendMsgQueryUnconfirmMsgReq,
    ) -> Result<crate::response_vo::mqtt::SendMsgQueryUnconfirmMsgRes, crate::Error> {
        let res = self
            .client
            .post("sendMsg/queryUnConfirmMsg")
            .json(serde_json::json!(req))
            .send::<BackendResponse>()
            .await?;

        res.process()
    }
}

#[cfg(test)]
mod test {

    use wallet_utils::init_test_log;

    use crate::{
        api::BackendApi,
        request::{SendMsgConfirm, SendMsgConfirmReq},
    };

    #[tokio::test]
    async fn test_send_msg_confirm() {
        init_test_log();
        // let method = "POST";
        let base_url = crate::consts::BASE_URL;

        let req = SendMsgConfirmReq {
            list: vec![SendMsgConfirm {
                id: "676059fcab8ff576d42076ef".to_string(),
                source: "MQTT".to_string(),
            }],
        };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .send_msg_confirm(&req)
            .await;

        println!("[test_chain_default_list] res: {res:?}");
    }

    #[tokio::test]
    async fn test_send_msg_query_unconfirm_msg() {
        init_test_log();
        // let method = "POST";
        let base_url = crate::consts::BASE_URL;
        // let client_id = "b205d2716d87d73af508ff2375149487".to_string();
        let client_id = "7552bd49a9407eb98164c129d11da7e2".to_string();
        // let client_id = "5cab3d9aeacd8e4996245806da44fd37".to_string();
        let req = crate::request::SendMsgQueryUnconfirmMsgReq { client_id };
        let res = BackendApi::new(Some(base_url.to_string()), None)
            .unwrap()
            .send_msg_query_unconfirm_msg(&req)
            .await;
        println!("[test_chain_default_list] res: {res:?}");
    }
}
